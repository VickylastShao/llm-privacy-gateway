#![forbid(unsafe_code)]

mod eval;

use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
    process::ExitCode,
};

use clap::{Args, Parser, Subcommand};
use gateway::{GatewayConfig, GatewayProtocol, GatewayState, RuntimeProfile, start_with_state};
use privacy_core::{
    BankCardDetector, ChinaIdDetector, CredentialsDetector, EntityKind, NetworkDetector,
    PhoneDetector, Policy, PrivacyEngine, StaticDictionary, UnifiedSocialCreditCodeDetector,
};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use session_map::{MemoryBackend, PseudonymMap};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
    name = "lpg-cli",
    about = "Local development tools for LLM Privacy Gateway"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Detect(TextArgs),
    Transform(MappingArgs),
    Restore(MappingArgs),
    Eval { path: PathBuf },
    Serve(ServeArgs),
}

#[derive(Debug, Args)]
struct TextArgs {
    /// Text to inspect. Reads stdin when omitted.
    text: Option<String>,
    /// Include matching source values in development output.
    #[arg(long)]
    show_sensitive_values: bool,
}

#[derive(Debug, Args)]
struct MappingArgs {
    /// Text to transform or restore. Reads stdin when omitted.
    text: Option<String>,
    /// Path for encrypted mapping records; required for cross-process restoration.
    #[arg(long)]
    mapping_file: PathBuf,
    /// 32-byte mapping key encoded as exactly 64 hexadecimal characters.
    #[arg(long)]
    mapping_key: String,
}

#[derive(Debug, Args)]
struct ServeArgs {
    #[arg(long, default_value = "127.0.0.1:17891")]
    bind: std::net::SocketAddr,
    #[arg(long, value_parser = ["openai", "anthropic"])]
    protocol: String,
    #[arg(long)]
    upstream_base_url: String,
    #[arg(long, default_value = "LPG_UPSTREAM_API_KEY")]
    upstream_key_env: String,
    #[arg(long, default_value = "LPG_LOCAL_API_KEY")]
    local_key_env: String,
    #[arg(long, default_value = "LPG_MAPPING_KEY_HEX")]
    mapping_key_env: String,
}

#[derive(Debug, Serialize)]
struct DetectionOutput<'a> {
    kind: String,
    start: usize,
    end: usize,
    source: &'a str,
    confidence: u8,
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'a str>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MappingFile {
    profile_id: Uuid,
    backend: MemoryBackend,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("lpg-cli: {error}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, String> {
    match cli.command {
        Command::Detect(args) => detect(read_text(args.text)?, args.show_sensitive_values),
        Command::Transform(mut args) => {
            let text = read_text(args.text.take())?;
            transform(text, args)
        }
        Command::Restore(mut args) => {
            let text = read_text(args.text.take())?;
            restore(text, args)
        }
        Command::Eval { path } => eval::run(&path),
        Command::Serve(args) => serve(args),
    }
}

fn serve(args: ServeArgs) -> Result<ExitCode, String> {
    let upstream_key = std::env::var(&args.upstream_key_env)
        .map_err(|_| format!("missing {}", args.upstream_key_env))?;
    let local_key = std::env::var(&args.local_key_env)
        .map_err(|_| format!("missing {}", args.local_key_env))?;
    let mapping_key = decode_key(
        &std::env::var(&args.mapping_key_env)
            .map_err(|_| format!("missing {}", args.mapping_key_env))?,
    )?;
    let protocol = match args.protocol.as_str() {
        "openai" => GatewayProtocol::OpenAi,
        "anthropic" => GatewayProtocol::Anthropic,
        _ => return Err("unsupported protocol".to_owned()),
    };
    let profile = RuntimeProfile::new(
        Uuid::new_v4(),
        protocol,
        local_key,
        args.upstream_base_url,
        SecretString::from(upstream_key),
        mapping_key,
    );
    let state = GatewayState::new([profile]).map_err(|error| error.to_string())?;
    let config = GatewayConfig::new(args.bind).map_err(|error| error.to_string())?;
    tokio::runtime::Runtime::new()
        .map_err(|error| error.to_string())?
        .block_on(start_with_state(config, state))
        .map_err(|error| error.to_string())?;
    Ok(ExitCode::SUCCESS)
}

fn engine() -> PrivacyEngine {
    PrivacyEngine::new(
        vec![
            Box::new(CredentialsDetector),
            Box::new(ChinaIdDetector),
            Box::new(UnifiedSocialCreditCodeDetector),
            Box::new(BankCardDetector),
            Box::new(PhoneDetector),
            Box::new(NetworkDetector),
        ],
        StaticDictionary::empty(),
    )
}

fn detect(text: String, show_values: bool) -> Result<ExitCode, String> {
    let inspection = engine()
        .inspect(&text, &Policy::strict())
        .map_err(|error| error.to_string())?;
    let mut output = Vec::new();
    for decision in inspection.decisions() {
        let detection = decision.detection();
        let value = if show_values {
            text.get(detection.range())
        } else {
            None
        };
        output.push(DetectionOutput {
            kind: entity_kind_name(detection.kind()),
            start: detection.start(),
            end: detection.end(),
            source: detection.source(),
            confidence: detection.confidence(),
            action: format!("{:?}", decision.action()),
            value,
        });
    }
    print_json(&output)?;
    Ok(ExitCode::SUCCESS)
}

fn transform(text: String, args: MappingArgs) -> Result<ExitCode, String> {
    let key = decode_key(&args.mapping_key)?;
    let mapping_file = load_mapping_file(&args.mapping_file)?;
    let mut map = PseudonymMap::new(mapping_file.profile_id, key, mapping_file.backend);
    match engine().transform(&text, &Policy::strict(), &mut map) {
        Ok(result) => {
            save_mapping_file(
                &args.mapping_file,
                &MappingFile {
                    profile_id: mapping_file.profile_id,
                    backend: map.backend().clone(),
                },
            )?;
            print_json(&serde_json::json!({"outbound_text": result.outbound_text()}))?;
            Ok(ExitCode::SUCCESS)
        }
        Err(privacy_core::PrivacyError::Blocked) => {
            print_json(&serde_json::json!({"error": "blocked"}))?;
            Ok(ExitCode::from(2))
        }
        Err(error) => Err(error.to_string()),
    }
}

fn restore(text: String, args: MappingArgs) -> Result<ExitCode, String> {
    let key = decode_key(&args.mapping_key)?;
    let mapping_file = load_mapping_file(&args.mapping_file)?;
    let map = PseudonymMap::new(mapping_file.profile_id, key, mapping_file.backend);
    let restored_text = map.restore_text(&text).map_err(|error| error.to_string())?;
    print_json(&serde_json::json!({"restored_text": restored_text}))?;
    Ok(ExitCode::SUCCESS)
}

fn load_mapping_file(path: &PathBuf) -> Result<MappingFile, String> {
    if !path.exists() {
        return Ok(MappingFile {
            profile_id: Uuid::new_v4(),
            backend: MemoryBackend::default(),
        });
    }
    let bytes = fs::read(path).map_err(|error| format!("cannot read mapping file: {error}"))?;
    if bytes.is_empty() {
        return Ok(MappingFile {
            profile_id: Uuid::new_v4(),
            backend: MemoryBackend::default(),
        });
    }
    serde_json::from_slice(&bytes).map_err(|_| "mapping file is invalid".to_owned())
}

fn save_mapping_file(path: &PathBuf, mapping: &MappingFile) -> Result<(), String> {
    let serialized =
        serde_json::to_vec(mapping).map_err(|_| "cannot serialize mapping file".to_owned())?;
    fs::write(path, serialized).map_err(|error| format!("cannot write mapping file: {error}"))
}

fn decode_key(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 {
        return Err("mapping key must be exactly 64 hexadecimal characters".to_owned());
    }
    let mut key = [0_u8; 32];
    for (index, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| "mapping key must be hexadecimal".to_owned())?;
    }
    Ok(key)
}

fn entity_kind_name(kind: &EntityKind) -> String {
    match kind {
        EntityKind::Custom(name) => format!("Custom({name})"),
        _ => format!("{kind:?}"),
    }
}

fn read_text(argument: Option<String>) -> Result<String, String> {
    match argument {
        Some(text) => Ok(text),
        None => {
            let mut text = String::new();
            io::stdin()
                .read_to_string(&mut text)
                .map_err(|error| error.to_string())?;
            Ok(text)
        }
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<(), String> {
    let encoded = serde_json::to_string(value).map_err(|error| error.to_string())?;
    println!("{encoded}");
    Ok(())
}

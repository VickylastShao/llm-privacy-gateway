use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use privacy_core::{
    BankCardDetector, ChinaIdDetector, CredentialsDetector, NetworkDetector, Policy, PrivacyEngine,
    StaticDictionary, UnifiedSocialCreditCodeDetector,
};

fn engine(dictionary_size: usize) -> PrivacyEngine {
    let terms = (0..dictionary_size).map(|index| format!("项目代号-{index:05}"));
    let dictionary =
        StaticDictionary::new(terms).unwrap_or_else(|error| panic!("dictionary: {error}"));
    PrivacyEngine::new(
        vec![
            Box::new(CredentialsDetector),
            Box::new(ChinaIdDetector),
            Box::new(UnifiedSocialCreditCodeDetector),
            Box::new(BankCardDetector),
            Box::new(NetworkDetector),
        ],
        dictionary,
    )
}

fn mixed_text(target_bytes: usize, hit: bool) -> String {
    let mut text = String::new();
    while text.len() < target_bytes {
        text.push_str("火电机组运行报告 contains ordinary engineering data. ");
    }
    if hit {
        text.push_str(" 项目代号-00001");
    }
    text
}

fn scan_benchmark(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("privacy_scan");
    for dictionary_size in [10, 1_000, 10_000] {
        let engine = engine(dictionary_size);
        for bytes in [1_024, 10_240, 102_400] {
            for hit in [false, true] {
                let text = mixed_text(bytes, hit);
                group.throughput(Throughput::Bytes(text.len() as u64));
                group.bench_with_input(
                    BenchmarkId::new(format!("dict_{dictionary_size}_hit_{hit}"), bytes),
                    &text,
                    |bench, input| {
                        bench.iter(|| {
                            engine
                                .inspect(input, &Policy::strict())
                                .unwrap_or_else(|error| panic!("scan: {error}"))
                        })
                    },
                );
            }
        }
    }
    group.finish();
}

criterion_group!(benches, scan_benchmark);
criterion_main!(benches);

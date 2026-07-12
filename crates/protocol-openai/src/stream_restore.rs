use session_map::{MapError, MappingBackend, PseudonymMap};

const PREFIX: &str = "__LPG_";

#[derive(Debug, Default)]
pub struct StreamRestorer {
    pending: String,
}

impl StreamRestorer {
    pub fn push<B: MappingBackend>(
        &mut self,
        chunk: &str,
        map: &PseudonymMap<B>,
    ) -> Result<String, MapError> {
        self.pending.push_str(chunk);
        let mut output = String::new();
        loop {
            let Some(start) = self.pending.find(PREFIX) else {
                let keep = suffix_prefix_length(&self.pending);
                let split = self.pending.len() - keep;
                output.push_str(&self.pending[..split]);
                self.pending = self.pending[split..].to_owned();
                return Ok(output);
            };
            output.push_str(&self.pending[..start]);
            let after_prefix = start + PREFIX.len();
            let Some(relative_end) = self.pending[after_prefix..].find("__") else {
                self.pending = self.pending[start..].to_owned();
                return Ok(output);
            };
            let end = after_prefix + relative_end + 2;
            let token = &self.pending[start..end];
            output.push_str(&map.restore_text(token)?);
            self.pending = self.pending[end..].to_owned();
        }
    }

    #[must_use]
    pub fn finish(mut self) -> String {
        std::mem::take(&mut self.pending)
    }
}

fn suffix_prefix_length(value: &str) -> usize {
    (1..PREFIX.len())
        .rev()
        .find(|&length| value.ends_with(&PREFIX[..length]))
        .unwrap_or(0)
}

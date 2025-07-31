const OUT_TAG: &str = "_out";

pub struct StreamingParser {
    buffer: String,
    tag_name: Option<String>,
}

impl StreamingParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            tag_name: None,
        }
    }

    pub fn feed(&mut self, chunk: &str) -> String {
        self.buffer.push_str(chunk);

        let mut output = String::new();

        while let Some(next_lt) = self.buffer.find('<') {
            if self.tag_name.as_deref() == Some(OUT_TAG) {
                output.push_str(&self.buffer[..next_lt]);
            }

            let next_gt = match self.buffer[next_lt..].find('>') {
                Some(i) => next_lt + i,
                None => break,
            };

            let is_closing = self.buffer[next_lt + 1..].starts_with('/');
            let tag_start = if is_closing { next_lt + 2 } else { next_lt + 1 };
            let tag = &self.buffer[tag_start..next_gt];

            if !is_closing && self.tag_name.is_none() && tag == OUT_TAG {
                self.tag_name = Some(tag.to_string());
            } else if is_closing && self.tag_name.as_deref() == Some(tag) {
                self.tag_name = None;
            }

            let drain_end = next_gt + 1;
            self.buffer.drain(..drain_end);
        }

        if self.tag_name.as_deref() == Some(OUT_TAG) {
            output.push_str(&self.buffer);
            self.buffer.clear();
        }

        output
    }
}

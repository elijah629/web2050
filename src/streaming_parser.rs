//! shobby xml parser
const OUT_TAG: &str = "_out";

pub struct StreamingParser {
    buffer: String,
    tag_depth: usize,
    top_level_tag_name: Option<String>,
}

impl StreamingParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            top_level_tag_name: None,
            tag_depth: 0,
        }
    }

    pub fn feed(&mut self, chunk: &str) -> String {
        self.buffer.push_str(chunk);

        let mut output = String::new();

        loop {
            let start = match self.buffer.find('<') {
                Some(i) => i,
                None => {
                    if self.top_level_tag_name.as_deref() == Some(OUT_TAG) && self.tag_depth > 0 {
                        output.extend(self.buffer.drain(..));
                    }
                    break;
                }
            };

            let rel_end = match self.buffer[start..].find('>') {
                Some(i) => i,
                None => {
                    break;
                }
            };
            let end = start + rel_end;

            let raw_tag = self.buffer[start..=end].to_string();

            let inner = &raw_tag[1..raw_tag.len() - 1];
            let is_closing = inner.starts_with('/');
            let tag_name = if is_closing { &inner[1..] } else { inner };

            if start > 0 {
                if self.top_level_tag_name.as_deref() == Some(OUT_TAG) && self.tag_depth > 0 {
                    output.extend(self.buffer.drain(..start));
                } else {
                    self.buffer.drain(..start);
                }
            }

            if self.top_level_tag_name.as_deref() == Some(OUT_TAG) && tag_name != OUT_TAG {
                output.push_str(&raw_tag);
            }

            if !is_closing {
                if self.tag_depth == 0 {
                    self.top_level_tag_name = Some(tag_name.to_string());
                    self.tag_depth = 1;
                } else if self.top_level_tag_name.as_deref() == Some(tag_name) {
                    self.tag_depth += 1;
                }
            } else if self.tag_depth > 0 && self.top_level_tag_name.as_deref() == Some(tag_name) {
                self.tag_depth -= 1;
                if self.tag_depth == 0 {
                    self.top_level_tag_name = None;
                }
            }

            self.buffer.drain(..=end - start);
        }

        output
    }
}

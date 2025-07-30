const OUT_TAG: &str = "_out";

pub struct StreamingParser {
    buffer: String,       // buffer to hold everything
    pending_text: String, // buffer to hold the output we are pending to output
    tag_depth: usize,     // how many tags are we inside of? depth
}

impl StreamingParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            pending_text: String::new(),
            tag_depth: 0,
        }
    }

    pub fn feed(&mut self, chunk: &str) -> String {
        self.buffer.push_str(chunk);
        let mut output = String::new();

        let mut pos = 0;
        let buf = &self.buffer;

        while let Some(start) = buf[pos..].find('<') {
            let start = pos + start;

            if self.tag_depth == 1 && start > pos {
                self.pending_text.push_str(&buf[pos..start]);
            }

            if let Some(end) = buf[start..].find('>') {
                let end = start + end;

                let tag_content = buf[start + 1..end].trim();
                let is_closing = tag_content.starts_with('/');
                let tag_name = if is_closing {
                    &tag_content[1..]
                } else {
                    tag_content
                };

                if tag_name == OUT_TAG {
                    if is_closing {
                        if !self.pending_text.is_empty() {
                            output.push_str(&self.pending_text);
                            self.pending_text.clear();
                        }
                        self.tag_depth = self.tag_depth.saturating_sub(1);
                    } else {
                        self.tag_depth += 1;
                    }
                }

                pos = end + 1;
            } else {
                break;
            }
        }

        if self.tag_depth == 1 && pos < buf.len() && !buf[pos..].starts_with('<') {
            self.pending_text.push_str(&buf[pos..]);
            pos = buf.len();
        }

        self.buffer.drain(..pos);

        if self.tag_depth == 1 && !self.pending_text.is_empty() {
            output.push_str(&self.pending_text);
            self.pending_text.clear();
        }

        output
    }
}

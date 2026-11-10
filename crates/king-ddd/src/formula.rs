//! Small bounded arithmetic language. A formula is data, never executable code.
use serde_json::{Value, json};
pub fn compile_formula(source: &str, names: &[String]) -> Result<Value, String> {
    if source.len() > 500 {
        return Err("Formula limit: 500 characters".into());
    }
    struct Parser<'a> {
        text: &'a [u8],
        at: usize,
        nodes: usize,
        names: &'a [String],
    }
    impl Parser<'_> {
        fn space(&mut self) {
            while self.text.get(self.at).is_some_and(u8::is_ascii_whitespace) {
                self.at += 1;
            }
        }
        fn take(&mut self, ch: u8) -> bool {
            self.space();
            if self.text.get(self.at) == Some(&ch) {
                self.at += 1;
                true
            } else {
                false
            }
        }
        fn parse(&mut self, min: u8, depth: usize) -> Result<Value, String> {
            self.nodes += 1;
            if depth > 32 || self.nodes > 200 {
                return Err("Formula is too complex".into());
            }
            self.space();
            let mut node = match self.text.get(self.at).copied() {
                Some(sign @ (b'+' | b'-')) => {
                    self.at += 1;
                    json!({"op":"unary","sign":(sign as char).to_string(),"value":self.parse(3,depth+1)?})
                }
                Some(b'(') => {
                    self.at += 1;
                    let n = self.parse(0, depth + 1)?;
                    if !self.take(b')') {
                        return Err("Missing closing parenthesis".into());
                    }
                    n
                }
                Some(b'[') => {
                    self.at += 1;
                    let start = self.at;
                    while self.text.get(self.at).is_some_and(|c| *c != b']') {
                        self.at += 1;
                    }
                    let name = std::str::from_utf8(&self.text[start..self.at])
                        .map_err(|e| e.to_string())?;
                    if !self.take(b']') {
                        return Err("Missing ]".into());
                    }
                    if !self.names.iter().any(|n| n == name) {
                        return Err(format!("Unknown numeric source column: {name}"));
                    }
                    json!({"op":"ref","name":name})
                }
                Some(c) if c.is_ascii_digit() || c == b'.' => {
                    let start = self.at;
                    while self
                        .text
                        .get(self.at)
                        .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
                    {
                        self.at += 1;
                    }
                    if self
                        .text
                        .get(self.at)
                        .is_some_and(|c| *c == b'e' || *c == b'E')
                    {
                        self.at += 1;
                        if self
                            .text
                            .get(self.at)
                            .is_some_and(|c| *c == b'+' || *c == b'-')
                        {
                            self.at += 1;
                        }
                        while self.text.get(self.at).is_some_and(u8::is_ascii_digit) {
                            self.at += 1;
                        }
                    }
                    let v = std::str::from_utf8(&self.text[start..self.at])
                        .map_err(|e| e.to_string())?
                        .parse::<f64>()
                        .map_err(|e| e.to_string())?;
                    if !v.is_finite() {
                        return Err("Use finite constants".into());
                    }
                    json!({"op":"number","v":v})
                }
                Some(c) if c.is_ascii_alphabetic() => {
                    let start = self.at;
                    while self.text.get(self.at).is_some_and(u8::is_ascii_alphabetic) {
                        self.at += 1;
                    }
                    let name = std::str::from_utf8(&self.text[start..self.at])
                        .map_err(|e| e.to_string())?
                        .to_string();
                    if !["abs", "min", "max", "coalesce"].contains(&name.as_str())
                        || !self.take(b'(')
                    {
                        return Err("Use abs, min, max or coalesce with parentheses".into());
                    }
                    let mut args = vec![self.parse(0, depth + 1)?];
                    while self.take(b',') {
                        args.push(self.parse(0, depth + 1)?);
                    }
                    if !self.take(b')') || args.len() != if name == "abs" { 1 } else { 2 } {
                        return Err("Wrong function arguments".into());
                    }
                    json!({"op":"call","name":name,"args":args})
                }
                _ => {
                    return Err(
                        "Use numbers, [column names], + - * /, abs, min, max or coalesce".into(),
                    );
                }
            };
            loop {
                self.space();
                let op = self.text.get(self.at).copied().unwrap_or(0);
                let prec = match op {
                    b'+' | b'-' => 1,
                    b'*' | b'/' => 2,
                    _ => 0,
                };
                if prec == 0 || prec < min {
                    break;
                }
                self.at += 1;
                node = json!({"op":(op as char).to_string(),"left":node,"right":self.parse(prec+1,depth+1)?});
            }
            Ok(node)
        }
    }
    let mut p = Parser {
        text: source.as_bytes(),
        at: 0,
        nodes: 0,
        names,
    };
    let tree = p.parse(0, 0)?;
    p.space();
    if p.at != p.text.len() {
        return Err("Unexpected formula text".into());
    }
    Ok(tree)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precedence_and_missing() {
        let tree = compile_formula("coalesce([x], 0) + 2 * 3", &["x".into()]).unwrap();
        assert_eq!(
            crate::constraints::eval_node(&tree, &json!({"x":""}), 0, &mut 0).unwrap(),
            Some(6.)
        );
        assert!(compile_formula("fetch([x])", &["x".into()]).is_err());
    }
}

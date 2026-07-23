use super::EqlSqlError;

#[derive(Debug, PartialEq)]
enum Tok {
    Word(String),   // [A-Za-z0-9_.] runs (covers numbers, idents, hex, ens)
    Other(String),  // whitespace, operators, punctuation
    Quoted(String), // complete '…' / "…" / -- … / /* … */ span, verbatim
}

fn tokenize(input: &str) -> Vec<Tok> {
    let chars: Vec<char> = input.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' || c == '"' {
            let quote = c;
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == quote {
                    // SQL escaping: '' inside '...' or "" inside "..."
                    if chars.get(i + 1) == Some(&quote) {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            toks.push(Tok::Quoted(chars[start..i].iter().collect()));
        } else if c == '-' && chars.get(i + 1) == Some(&'-') {
            let start = i;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            toks.push(Tok::Quoted(chars[start..i].iter().collect()));
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            let start = i;
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            toks.push(Tok::Quoted(chars[start..i].iter().collect()));
        } else if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            toks.push(Tok::Word(chars[start..i].iter().collect()));
        } else {
            let start = i;
            i += 1;
            toks.push(Tok::Other(chars[start..i].iter().collect()));
        }
    }
    toks
}

fn is_hex(w: &str) -> bool {
    w.len() > 2 && w.starts_with("0x") && w[2..].chars().all(|c| c.is_ascii_hexdigit())
}

fn is_ens(w: &str) -> bool {
    let mut segs = w.split('.').collect::<Vec<_>>();
    if segs.len() < 2 || segs.pop() != Some("eth") {
        return false;
    }
    segs.iter()
        .all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

fn is_number(w: &str) -> bool {
    !w.is_empty()
        && w.chars().all(|c| c.is_ascii_digit() || c == '.')
        && w.chars().filter(|c| *c == '.').count() <= 1
        && w.chars().any(|c| c.is_ascii_digit())
}

fn unit_multiplier(w: &str) -> Option<u32> {
    // returns the power of ten
    match w.to_ascii_lowercase().as_str() {
        "ether" => Some(18),
        "gwei" => Some(9),
        "wei" => Some(0),
        _ => None,
    }
}

fn fold_unit(number: &str, pow: u32) -> Result<String, EqlSqlError> {
    let (int_part, frac_part) = match number.split_once('.') {
        Some((i, f)) => (i, f),
        None => (number, ""),
    };
    if frac_part.len() as u32 > pow {
        return Err(EqlSqlError::Validation(format!(
            "{number} with this unit is not a whole number of wei"
        )));
    }
    let zeros = pow as usize - frac_part.len();
    let digits = format!("{int_part}{frac_part}{}", "0".repeat(zeros));
    let trimmed = digits.trim_start_matches('0');
    Ok(if trimmed.is_empty() {
        "0".into()
    } else {
        trimmed.into()
    })
}

pub fn prelex(input: &str) -> Result<String, EqlSqlError> {
    let toks = tokenize(input);
    let mut out = String::new();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Quoted(s) | Tok::Other(s) => out.push_str(s),
            Tok::Word(w) => {
                // number followed by a unit word (skipping whitespace-only Others)?
                if is_number(w) {
                    let mut j = i + 1;
                    let mut ws = String::new();
                    while let Some(Tok::Other(o)) = toks.get(j) {
                        if o.chars().all(char::is_whitespace) {
                            ws.push_str(o);
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    if let Some(Tok::Word(u)) = toks.get(j) {
                        if let Some(pow) = unit_multiplier(u) {
                            out.push_str(&fold_unit(w, pow)?);
                            i = j + 1;
                            continue;
                        }
                    }
                    let _ = ws; // no unit: fall through, number printed as-is
                }
                if is_hex(w) || is_ens(w) {
                    out.push('\'');
                    out.push_str(w);
                    out.push('\'');
                } else {
                    out.push_str(w);
                }
            }
        }
        i += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::prelex;

    #[test]
    fn quotes_bare_hex() {
        assert_eq!(
            prelex("WHERE address = 0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045").unwrap(),
            "WHERE address = '0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045'"
        );
    }

    #[test]
    fn quotes_ens_names_and_subdomains() {
        assert_eq!(
            prelex("address = vitalik.eth").unwrap(),
            "address = 'vitalik.eth'"
        );
        assert_eq!(
            prelex("address = sub.vitalik.eth").unwrap(),
            "address = 'sub.vitalik.eth'"
        );
    }

    #[test]
    fn leaves_plain_identifiers_alone() {
        assert_eq!(
            prelex("chain = eth AND number = latest").unwrap(),
            "chain = eth AND number = latest"
        );
    }

    #[test]
    fn folds_units() {
        assert_eq!(
            prelex("value > 1 ether").unwrap(),
            "value > 1000000000000000000"
        );
        assert_eq!(
            prelex("gas_price < 30 gwei").unwrap(),
            "gas_price < 30000000000"
        );
        assert_eq!(
            prelex("value > 1.5 ether").unwrap(),
            "value > 1500000000000000000"
        );
        assert_eq!(prelex("value = 10 wei").unwrap(), "value = 10");
    }

    #[test]
    fn fractional_wei_is_an_error() {
        assert!(prelex("value > 1.5 wei").is_err());
    }

    #[test]
    fn does_not_touch_strings_or_comments() {
        assert_eq!(
            prelex("sig = 'Transfer(address,address,uint256)' -- 1 ether").unwrap(),
            "sig = 'Transfer(address,address,uint256)' -- 1 ether"
        );
        assert_eq!(
            prelex("name = 'my-name.eth'").unwrap(),
            "name = 'my-name.eth'"
        );
    }

    #[test]
    fn hex_inside_in_list() {
        assert_eq!(
            prelex("address IN (0xAb, vitalik.eth)").unwrap(),
            "address IN ('0xAb', 'vitalik.eth')"
        );
    }
}

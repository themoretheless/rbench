//! Second-consumer microbench: UTF-8 JSON-ish tokenization throughput.
//!
//! Demonstrates Suite + work units + optional OS process metrics outside timing.
//! This is not a JSON parser correctness suite — it counts structural tokens.
use rbench::{DropPolicy, Suite};

fn tokens(input: &str) -> usize {
    let mut n = 0usize;
    let mut in_string = false;
    let mut escape = false;
    for b in input.bytes() {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
                n += 1;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' | b'}' | b'[' | b']' | b':' | b',' => n += 1,
            b'0'..=b'9' | b'-' => n += 1, // coarse: one token per digit/sign run start handled below
            _ => {}
        }
    }
    // Count number-like runs properly
    let mut i = 0;
    let bytes = input.as_bytes();
    let mut numbers = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if !in_string_at(bytes, i)
            && (b.is_ascii_digit() || (b == b'-' && bytes.get(i + 1).is_some_and(|c| c.is_ascii_digit())))
        {
            numbers += 1;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.' || bytes[i] == b'e' || bytes[i] == b'E' || bytes[i] == b'+' || bytes[i] == b'-')
            {
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    // Prefer the structural+string count; add numbers if the coarse digit loop over-counted.
    n.max(numbers)
}

fn in_string_at(bytes: &[u8], at: usize) -> bool {
    let mut in_string = false;
    let mut escape = false;
    for &b in &bytes[..at] {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
        } else if b == b'"' {
            in_string = true;
        }
    }
    in_string
}

fn fixture(size: usize) -> String {
    let mut out = String::from("{\"items\":[");
    for i in 0..size {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"id\":{i},\"name\":\"item-{i}\",\"ok\":true,\"n\":{}}}",
            i * 3
        ));
    }
    out.push_str("]}");
    out
}

fn main() -> rbench::Result<()> {
    let mut suite = Suite::new("tokenizer");
    suite.process_metrics(true);
    for size in [32usize, 256, 1024] {
        let input = fixture(size);
        let bytes = input.len() as u64;
        suite
            .bench_with_input(
                &format!("jsonish/{size}"),
                move || input.clone(),
                |s| {
                    let n = tokens(s.as_str());
                    assert!(n > 0);
                    n
                },
                DropPolicy::OutsideTiming,
            )
            .parameter("objects", size)
            .work_units("bytes", bytes)
            .tag("cpu")
            .tag("parser");
    }
    suite.main()
}

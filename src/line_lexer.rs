use anyhow::{bail, Ok, Result};

#[derive(Debug, Clone)]
pub struct EntryLineLexer<'a> {
    lines: Vec<&'a str>,
    index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EntryLine<'a> {
    Empty,
    Comment(&'a str),
    ProductionMarker,
    Header(&'a str),
    Option(&'a str, &'a str),
    LockedHeader(&'a str),
    LockedOption(&'a str, &'a str),
}

impl<'a> EntryLineLexer<'a> {
    pub fn new(contents: &'a str) -> Self {
        let lines = contents.lines().collect();
        Self { lines, index: 0 }
    }

    pub fn tokenize(&mut self) -> Result<Vec<EntryLine<'a>>> {
        let mut res = vec![];
        while let Some(line) = self.next_line() {
            if line.trim().starts_with('#') {
                res.push(tokenize_commented(line));
            } else {
                res.push(tokenize_uncommented(line)?);
            }
        }

        Ok(res)
    }

    fn next_line(&mut self) -> Option<&'a str> {
        let res = self.lines.get(self.index).copied();
        if self.index < self.lines.len() {
            self.index += 1;
        }
        res
    }
}

fn tokenize_commented(line: &str) -> EntryLine {
    let line = line.trim()[1..].trim();
    if line == "production" {
        EntryLine::ProductionMarker
    } else if line.starts_with('[') && line.ends_with(']') {
        // Header
        EntryLine::LockedHeader(&line[1..line.len() - 1])
    } else if line.contains('=') {
        // Option
        let [key, value]: [&str; 2] = line
            .splitn(2, '=')
            .collect::<Vec<_>>()
            .try_into()
            .expect("should always be splitted to two entries");
        EntryLine::LockedOption(key, value)
    } else {
        // Simple Comment
        EntryLine::Comment(&line[1..])
    }
}

fn tokenize_uncommented(line: &str) -> Result<EntryLine> {
    if line.starts_with('[') && line.ends_with(']') {
        // Header
        Ok(EntryLine::Header(&line[1..line.len() - 1]))
    } else if line.contains('=') {
        // Option
        let [key, value]: [&str; 2] = line
            .splitn(2, '=')
            .collect::<Vec<_>>()
            .try_into()
            .expect("should always be splitted to two entries");
        let key = key.trim_end();
        let value = value.trim_start();
        Ok(EntryLine::Option(key, value))
    } else {
        bail!("unexpected line: {:?}", line);
    }
}

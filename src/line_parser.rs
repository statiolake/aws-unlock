use anyhow::{bail, Ok, Result};
use std::{collections::HashMap, iter::from_fn};

use crate::line_lexer::EntryLine;

#[derive(Debug, Clone)]
pub struct EntryLineParser<'a> {
    lines: Vec<EntryLine<'a>>,
    index: usize,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub comments: Vec<String>,
    pub is_production: bool,
    pub is_locked: bool,
    pub header: String,
    pub values: HashMap<String, String>,
}

impl<'a> EntryLineParser<'a> {
    pub fn new(lines: Vec<EntryLine<'a>>) -> Self {
        Self { lines, index: 0 }
    }

    pub fn parse(&mut self) -> Result<Vec<Entry>> {
        from_fn(|| self.parse_one().transpose()).collect()
    }

    pub fn parse_one(&mut self) -> Result<Option<Entry>> {
        self.skip_empty_line();
        if self.is_finished() {
            return Ok(None);
        }

        let mut all_comments = vec![];
        let (comments, is_production) = self.parse_is_production()?;
        all_comments.extend(comments);

        let (comments, is_locked) = self.parse_is_locked()?;
        all_comments.extend(comments);

        let (comments, header) = self.parse_header(is_locked)?;
        all_comments.extend(comments);

        let (comments, values) = self.parse_values(is_locked)?;
        all_comments.extend(comments);

        Ok(Some(Entry {
            comments: all_comments,
            is_production,
            is_locked,
            header,
            values,
        }))
    }

    fn parse_is_production(&mut self) -> Result<(Vec<String>, bool)> {
        let mut comments = vec![];
        while let Some(line) = self.peek_line() {
            match line {
                EntryLine::Empty => {
                    self.next_line().unwrap();
                    continue;
                }
                EntryLine::Comment(comment) => {
                    let comment = comment.to_string();
                    self.next_line().unwrap();
                    comments.push(comment);
                    continue;
                }
                EntryLine::ProductionMarker => {
                    self.next_line().unwrap();
                    return Ok((comments, true));
                }
                _ => return Ok((comments, false)),
            }
        }

        bail!("unexpected EOF while scanning is_production");
    }

    fn parse_is_locked(&mut self) -> Result<(Vec<String>, bool)> {
        let mut comments = vec![];
        while let Some(line) = self.peek_line() {
            match line {
                EntryLine::Empty => {
                    self.next_line().unwrap();
                    continue;
                }
                EntryLine::Comment(comment) => {
                    let comment = comment.to_string();
                    self.next_line().unwrap();
                    comments.push(comment);
                    continue;
                }
                EntryLine::ProductionMarker => bail!("unexpected production marker"),
                EntryLine::LockedHeader(_) | EntryLine::LockedOption(_, _) => {
                    return Ok((comments, true))
                }
                EntryLine::Header(_) | EntryLine::Option(_, _) => return Ok((comments, false)),
            }
        }

        bail!("unexpected EOF while scanning is_locked");
    }

    fn parse_header(&mut self, is_locked: bool) -> Result<(Vec<String>, String)> {
        let mut comments = vec![];
        while let Some(line) = self.peek_line() {
            match line {
                EntryLine::Empty => {
                    self.next_line().unwrap();
                    continue;
                }
                EntryLine::Comment(comment) => {
                    let comment = comment.to_string();
                    self.next_line().unwrap();
                    comments.push(comment);
                    continue;
                }
                EntryLine::ProductionMarker => {
                    bail!("unexpected production marker while parsing header")
                }
                EntryLine::Header(header) if !is_locked => {
                    let header = header.to_string();
                    self.next_line().unwrap();
                    return Ok((comments, header));
                }
                EntryLine::LockedHeader(header) if is_locked => {
                    let header = header.to_string();
                    self.next_line().unwrap();
                    return Ok((comments, header));
                }
                _ => bail!("unexpected line while scanning header"),
            }
        }

        bail!("unexpected EOF while scanning header");
    }

    fn parse_values(&mut self, is_locked: bool) -> Result<(Vec<String>, HashMap<String, String>)> {
        let mut values = HashMap::new();
        while let Some(line) = self.peek_line() {
            match line {
                EntryLine::Empty => {
                    self.next_line().unwrap();
                    continue;
                }
                EntryLine::Option(key, value) if !is_locked => {
                    let key = key.to_string();
                    let value = value.to_string();
                    self.next_line().unwrap();
                    values.insert(key, value);
                }
                EntryLine::LockedOption(key, value) => {
                    let key = key.to_string();
                    let value = value.to_string();
                    self.next_line().unwrap();
                    values.insert(key, value);
                }
                EntryLine::ProductionMarker
                | EntryLine::Comment(_)
                | EntryLine::Header(_)
                | EntryLine::LockedHeader(_) => return Ok((vec![], values)),
                _ => bail!("unexpected line while scanning values"),
            }
        }

        Ok((vec![], values))
    }

    fn skip_empty_line(&mut self) {
        while self.peek_line() == Some(&EntryLine::Empty) {
            self.next_line().unwrap();
        }
    }

    fn is_finished(&self) -> bool {
        self.index == self.lines.len()
    }

    fn peek_line(&self) -> Option<&EntryLine> {
        self.lines.get(self.index)
    }

    fn next_line(&mut self) -> Option<&EntryLine> {
        let res = self.lines.get(self.index);
        if self.index < self.lines.len() {
            self.index += 1;
        }
        res
    }
}

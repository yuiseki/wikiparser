//! Wikimedia types
use std::{collections::HashSet, error::Error, ffi::OsStr, fmt::Display, fs, str::FromStr};

use anyhow::{anyhow, bail, Context};

mod page;
pub use page::Page;
mod title;
pub use title::*;
mod qid;
pub use qid::*;

/// Read from a file of urls on each line.
pub fn parse_wikidata_file(path: impl AsRef<OsStr>) -> anyhow::Result<HashSet<Qid>> {
    let contents = fs::read_to_string(path.as_ref())?;
    Ok(contents
        .lines()
        .enumerate()
        .map(|(i, line)| {
            Qid::from_str(line).with_context(|| {
                let line_num = i + 1;
                format!("on line {line_num}: {line:?}")
            })
        })
        .filter_map(|r| match r {
            Ok(qid) => Some(qid),
            Err(e) => {
                warn!("Could not parse QID: {:#}", e);
                None
            }
        })
        .collect())
}

/// Read article titles from a file of urls on each line.
pub fn parse_wikipedia_file(path: impl AsRef<OsStr>) -> anyhow::Result<HashSet<Title>> {
    let contents = fs::read_to_string(path.as_ref())?;
    Ok(contents
        .lines()
        .enumerate()
        .map(|(i, line)| {
            Title::from_url(line).with_context(|| {
                let line_num = i + 1;
                format!("on line {line_num}: {line:?}")
            })
        })
        .filter_map(|r| match r {
            Ok(qid) => Some(qid),
            Err(e) => {
                warn!("Could not parse wikipedia title: {:#}", e);
                None
            }
        })
        .collect())
}

pub fn parse_osm_tag_file(
    path: impl AsRef<OsStr>,
    qids: &mut HashSet<Qid>,
    titles: &mut HashSet<Title>,
    mut line_errors: Option<&mut Vec<ParseLineError>>,
) -> anyhow::Result<()> {
    let path = path.as_ref();
    let mut rdr = csv::ReaderBuilder::new().delimiter(b'\t').from_path(path)?;

    let mut push_error = |e: ParseLineError| {
        debug!("Tag parse error: {e}");
        if let Some(ref mut errs) = line_errors {
            errs.push(e);
        }
    };

    let mut qid_col = None;
    let mut title_col = None;
    let mut osm_id_col = None;
    for (column, title) in rdr.headers()?.iter().enumerate() {
        match title {
            "wikidata" => qid_col = Some(column),
            "wikipedia" => title_col = Some(column),
            "@id" => osm_id_col = Some(column),
            _ => (),
        }
    }

    let qid_col = qid_col.ok_or_else(|| anyhow!("Cannot find 'wikidata' column"))?;
    let title_col = title_col.ok_or_else(|| anyhow!("Cannot find 'wikipedia' column"))?;

    let mut row = csv::StringRecord::new();
    loop {
        match rdr.read_record(&mut row) {
            Ok(true) => {}
            // finished
            Ok(false) => break,
            // attempt to recover from parsing errors
            Err(e) => {
                if e.is_io_error() {
                    bail!(e)
                }
                push_error(ParseLineError {
                    text: String::new(),
                    line: rdr.position().line(),
                    osm_id: None,
                    kind: e.into(),
                });
                continue;
            }
        }

        let osm_id = osm_id_col.and_then(|i| row[i].parse().ok());

        let qid = &row[qid_col].trim();
        if !qid.is_empty() {
            match Qid::from_str(qid) {
                Ok(qid) => {
                    qids.insert(qid);
                }
                Err(e) => push_error(ParseLineError {
                    text: qid.to_string(),
                    line: rdr.position().line(),
                    osm_id,
                    kind: e.into(),
                }),
            }
        }

        let title = &row[title_col].trim();
        if !title.is_empty() {
            match Title::from_osm_tag(title) {
                Ok(title) => {
                    titles.insert(title);
                }
                Err(e) => push_error(ParseLineError {
                    text: title.to_string(),
                    line: rdr.position().line(),
                    osm_id,
                    kind: e.into(),
                }),
            }
        }
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum ParseErrorKind {
    #[error("title")]
    Title(#[from] ParseTitleError),
    #[error("QID")]
    Qid(#[from] ParseQidError),
    #[error("TSV line")]
    Tsv(#[from] csv::Error),
}

#[derive(Debug)]
pub struct ParseLineError {
    pub text: String,
    pub line: u64,
    pub osm_id: Option<usize>,
    pub kind: ParseErrorKind,
}

impl Display for ParseLineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "on line {}", self.line)?;
        if let Some(osm_id) = self.osm_id {
            write!(f, " ({osm_id})")?;
        }
        write!(f, ": {} {:?}", self.kind, self.text)?;

        // Write source error chain to ensure they are logged.
        let mut source = self.kind.source();
        while let Some(e) = source {
            write!(f, ": {}", e)?;
            source = e.source();
        }
        Ok(())
    }
}

impl Error for ParseLineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        // Return nothing because Display prints source chain.
        None
    }
}

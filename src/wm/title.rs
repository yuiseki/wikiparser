use std::{fmt::Display, path::PathBuf, string::FromUtf8Error};

use url::Url;

/// Normalized wikipedia article title that can compare:
/// - titles `Spatial Database`
/// - urls `https://en.wikipedia.org/wiki/Spatial_database#Geodatabase`
/// - osm-style tags `en:Spatial Database`
///
/// ```
/// use om_wikiparser::wm::Title;
///
/// let title = Title::from_title("Article Title", "en").unwrap();
/// let url = Title::from_url("https://en.wikipedia.org/wiki/Article_Title#Section").unwrap();
/// let mobile = Title::from_url("https://en.m.wikipedia.org/wiki/Article_Title#Section").unwrap();
/// let url_tag1 = Title::from_osm_tag("https://en.m.wikipedia.org/wiki/Article_Title#Section").unwrap();
/// let url_tag2 = Title::from_osm_tag("de:https://en.m.wikipedia.org/wiki/Article_Title#Section").unwrap();
/// assert_eq!(url, title);
/// assert_eq!(url, mobile);
/// assert_eq!(url, url_tag1);
/// assert_eq!(url, url_tag2);
///
/// assert!(Title::from_url("https://en.wikipedia.org/not_a_wiki_page").is_err());
/// assert!(Title::from_url("https://wikidata.org/wiki/Q12345").is_err());
///
/// assert!(
///     Title::from_url("https://de.wikipedia.org/wiki/Breil/Brigels").unwrap() !=
///     Title::from_url("https://de.wikipedia.org/wiki/Breil").unwrap()
/// );
/// ```
#[derive(Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct Title {
    lang: String,
    name: String,
}

impl Display for Title {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.lang, self.name)
    }
}

impl Title {
    fn normalize_title(title: &str) -> String {
        // TODO: Compare with map generator url creation, ensure covers all cases.
        title.trim().replace(' ', "_")
    }

    // https://en.wikipedia.org/wiki/Article_Title/More_Title
    pub fn from_url(url: &str) -> Result<Self, ParseTitleError> {
        let url = url.trim();
        if url.is_empty() {
            return Err(ParseTitleError::Empty);
        }

        let url = Url::parse(url)?;

        let (subdomain, host) = url
            .host_str()
            .ok_or(ParseTitleError::NoHost)?
            .split_once('.')
            .ok_or(ParseTitleError::NoSubdomain)?;
        let host = host.strip_prefix("m.").unwrap_or(host);
        if host != "wikipedia.org" {
            return Err(ParseTitleError::BadDomain);
        }
        let lang = subdomain;

        let path = url.path();

        let (root, title) = path
            .strip_prefix('/')
            .unwrap_or(path)
            .split_once('/')
            .ok_or(ParseTitleError::ShortPath)?;

        if root != "wiki" {
            return Err(ParseTitleError::BadPath);
        }
        let title = urlencoding::decode(title)?;

        Self::from_title(&title, lang)
    }

    // en:Article Title
    pub fn from_osm_tag(tag: &str) -> Result<Self, ParseTitleError> {
        let tag = tag.trim();
        if tag.is_empty() {
            return Err(ParseTitleError::Empty);
        }
        let (lang, title) = tag.split_once(':').ok_or(ParseTitleError::MissingColon)?;

        let lang = lang.trim_start();
        let title = title.trim_start();

        if matches!(lang, "http" | "https") {
            return Self::from_url(tag);
        }

        if title.starts_with("http://") || title.starts_with("https://") {
            return Self::from_url(title);
        }

        Self::from_title(title, lang)
    }

    pub fn from_title(title: &str, lang: &str) -> Result<Self, ParseTitleError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(ParseTitleError::NoTitle);
        }
        // Wikipedia titles must be less than 256 bytes of UTF-8.
        // See: https://en.wikipedia.org/wiki/Wikipedia:Naming_conventions_(technical_restrictions)#Title_length
        if !title.len() < 256 {
            return Err(ParseTitleError::TitleLong);
        }

        // TODO: titles have a number of restrictions, including containing percent-encoded characters
        // See <https://en.wikipedia.org/wiki/Wikipedia:Page_name#Technical_restrictions_and_limitations>

        // TODO: special titles in "namespaces" start with a word and colon. They should not be linked from OSM.
        // See <https://en.wikipedia.org/wiki/Wikipedia:Namespace>

        let lang = lang.trim();
        if lang.is_empty() {
            return Err(ParseTitleError::NoLang);
        }
        if lang.contains(|c: char| !(c.is_ascii_alphabetic() || c == '-')) {
            return Err(ParseTitleError::LangBadChar);
        }
        let lang = lang.to_ascii_lowercase();

        let name = Self::normalize_title(title);
        Ok(Self { name, lang })
    }

    pub fn get_dir(&self, base: PathBuf) -> PathBuf {
        let mut path = base;
        // TODO: can use as_mut_os_string with 1.70.0
        path.push(format!("{}.wikipedia.org", self.lang));
        path.push("wiki");
        path.push(&self.name);

        path
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseTitleError {
    #[error("value is empty or whitespace")]
    Empty,
    #[error("title is empty or whitespace")]
    NoTitle,
    #[error("title is too long")]
    TitleLong,
    #[error("lang is empty or whitespace")]
    NoLang,
    #[error("lang contains character that is not alphabetic or '-'")]
    LangBadChar,
    #[error("no ':' separating lang and title")]
    MissingColon,

    // url-specific
    #[error("cannot parse url")]
    Url(#[from] url::ParseError),
    #[error("cannot decode url")]
    UrlDecode(#[from] FromUtf8Error),
    #[error("no host in url")]
    NoHost,
    #[error("no subdomain in url")]
    NoSubdomain,
    #[error("url base domain is not wikipedia.org")]
    BadDomain,
    #[error("url base path is not /wiki/")]
    BadPath,
    #[error("path has less than 2 segments")]
    ShortPath,
}

use crate::app::bsky::feed::PostLabels;
use crate::app::bsky::{embed::Embeds, feed::EntityRef};
use crate::app::bsky::richtext::Facet;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "$type")]
#[serde(rename = "app.bbs.post")]
#[serde(rename_all = "camelCase")]
pub struct Post {
    /// Client-declared timestamp when this post was originally created.
    pub created_at: DateTime<Utc>,
    /// The primary post content. Might be an empty string, if there are embeds.
    pub text: String,
    /// DEPRECATED: replaced by app.bsky.richtext.facet.
    pub entities: Option<Vec<EntityRef>>,
    /// Annotations of text (mentions, URLs, hashtags, .etc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facets: Option<Vec<Facet>>,
    /// Indicates human language of post primary text content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub langs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<PostLabels>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embed: Option<Embeds>,
    /// Additional hashtags, in addition to any included in post text and facets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Chose bbs section
    pub section_id: usize,
    /// BBS Post title
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "$type")]
#[serde(rename = "app.bbs.reply")]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    /// Client-declared timestamp when this post was originally created.
    pub created_at: DateTime<Utc>,
    /// The primary post content. Might be an empty string, if there are embeds.
    pub text: String,
    /// DEPRECATED: replaced by app.bsky.richtext.facet.
    pub entities: Option<Vec<EntityRef>>,
    /// Annotations of text (mentions, URLs, hashtags, .etc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facets: Option<Vec<Facet>>,
    /// Indicates human language of post primary text content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub langs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<PostLabels>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embed: Option<Embeds>,
    /// Additional hashtags, in addition to any included in post text and facets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// reply root cid
    pub root: String,
    /// reply parent cid
    pub parent: String,
}

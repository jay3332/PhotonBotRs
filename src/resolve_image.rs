use regex::Regex;

use serenity::client::Context;
use serenity::model::{channel::{Attachment, Message}, guild::{Member, Emoji}, id::{GuildId, ChannelId}};
use serenity::framework::standard::CommandError;

use serenity::utils::ArgumentConvert;

pub const DEFAULT_MAX_WIDTH: usize = 2048;
pub const DEFAULT_MAX_HEIGHT: usize = DEFAULT_MAX_WIDTH;
pub const DEFAULT_MAX_SIZE: usize = 1024 * 1024 * 6;  // 6 MiB

lazy_static::lazy_static! {
    pub static ref URL_REGEX: Regex = Regex::new(r"https?://\S+").unwrap();
    pub static ref TENOR_REGEX: Regex = Regex::new(r"https?://(www\.)?tenor\.com/view/\S+/").unwrap();
    pub static ref GIPHY_REGEX: Regex = Regex::new(r"https?://(www\.)?giphy\.com/gifs/[A-Za-z0-9]+/?").unwrap();
    pub static ref EMOJI_REGEX: Regex = Regex::new(r"<(a)?:([a-zA-Z0-9_]{2,32}):([0-9]{17,25})>").unwrap();
}

pub const ALLOWED_CONTENT_TYPES: [&str; 4] = [
    "image/png",
    "image/jpeg",
    "image/jpg",
    "image/webp",
];

pub const ALLOWED_SUFFIXES: [&str; 4] = [
    ".png",
    ".jpeg",
    ".jpg",
    ".webp",
];

pub enum Query {
    String(String),
    Emoji(Emoji),
    Member(Member),
}

pub enum RawResult<'a> {
    Attachment(&'a Attachment),
    Bytes(Vec<u8>),
    Url(String),
}

pub struct ImageResolver {
    pub allow_gifs: bool,
    pub allow_user_avatars: bool,
    pub fallback_to_user_avatar: bool,
    pub run_conversions: bool,

    pub max_width: usize,
    pub max_height: usize,
    pub max_size: usize,
}

impl ImageResolver {
    pub fn new() -> Self {
        Self {
            allow_gifs: true,
            allow_user_avatars: true,
            fallback_to_user_avatar: true,
            run_conversions: true,
            max_width: DEFAULT_MAX_WIDTH,
            max_height: DEFAULT_MAX_HEIGHT,
            max_size: DEFAULT_MAX_SIZE,
        }
    }

    pub fn disallow_gifs(&mut self) -> &mut Self {
        self.allow_gifs = false;
        self
    }

    pub fn disallow_user_avatars(&mut self) -> &mut Self {
        self.allow_user_avatars = false;
        self
    }

    pub fn disable_fallback_to_user_avatar(&mut self) -> &mut Self {
        self.fallback_to_user_avatar = false;
        self
    }

    pub fn disable_conversions(&mut self) -> &mut Self {
        self.run_conversions = false;
        self
    }

    pub fn max_width(&mut self, width: usize) -> &mut Self {
        self.max_width = width;
        self
    }

    pub fn max_height(&mut self, height: usize) -> &mut Self {
        self.max_height = height;
        self
    }

    pub fn max_size(&mut self, size: usize) -> &mut Self {
        self.max_size = size;
        self
    }

    async fn _run_conversions(ctx: &Context, guild_id: Option<GuildId>, channel_id: Option<ChannelId>, query: String) -> Query {
        if let Ok(o) = Member::convert(ctx, guild_id, channel_id, &query).await {
            return Query::Member(o);
        }

        if let Ok(o) = Emoji::convert(ctx, guild_id, channel_id, &query).await {
            return Query::Emoji(o);
        }

        Query::String(query)
    }

    fn _humanize_size(mut size: f64) -> String {
        let units = ["B", "KB", "MB", "GB", "TB", "PB"];

        for unit in units {            
            if size < 1024.0 {
                return format!("{} {}", size, unit);
            }

            size /= 1024.0;
        }

        unreachable!()
    }

    fn _url_from_emoji(emoji: String) -> String {
        if let Some(c) = EMOJI_REGEX.captures_iter(&emoji).next() {
            let animated = c.get(1).map_or(false, |m| m.as_str() == "a");
            let id = c.get(3).unwrap().as_str();

            format!("https://cdn.discordapp.com/emojis/{}.{}?v=1", id, if animated { "gif" } else { "png" })
        }
        else {
            format!("https://emojicdn.elk.sh/{}?style=twitter", )
        }
    }

    async fn _scrape_tenor(&self, url: String) -> Result<String, CommandError> {
        let resp = reqwest::get(&url).await?;

        if resp.status().is_success() {
            Ok(resp
                .text()
                .await?
                .split("contentUrl")
                .nth(1)
                .unwrap()
                .split("content")
                .nth(0)
                .unwrap()[2..]
                .split("\"")
                .nth(1)
                .unwrap()
                .replace(r"\u002F", "/"))
        } else {
            Err(CommandError::from(format!("URL returned status code {}", resp.status())))
        }
    }

    async fn _scrape_giphy(&self, url: String) -> Result<String, CommandError> {
        let resp = reqwest::get(&url).await?;

        if resp.status().is_success() {
            Ok("https://media".to_string() + resp
                .text()
                .await?
                .split("https://media")
                .nth(2)
                .unwrap()
                .split("\"")
                .nth(0)
                .unwrap())
        } else {
            Err(CommandError::from(format!("URL returned status code {}", resp.status())))
        }
    }

    async fn _sanitize(&self, result: RawResult<'_>, allowed_content_types: &Vec<&str>, allowed_suffixes: &Vec<&str>) -> Result<Vec<u8>, CommandError> {
        match result {
            RawResult::Attachment(attachment) => {
                if allowed_suffixes.into_iter().any(|suff| !attachment.filename.ends_with(suff)) {
                    let suffix = attachment.filename.split(".").last().unwrap_or("unknown");
                    Err(CommandError::from(format!("File extension `{}` is not allowed", suffix)))
                }
                
                else if attachment.size > self.max_size as u64 {
                    Err(CommandError::from(format!(
                        "Attachment is too big. (`{}` > `{}`)",
                        Self::_humanize_size(attachment.size as f64),
                        Self::_humanize_size(self.max_size as f64),
                    )))
                }
                
                else if attachment.width.is_none() || attachment.height.is_none() {
                    Err(CommandError::from("Invalid attachment. (Could not get a width or height from it.)"))
                }
                
                else if attachment.width.unwrap() > self.max_width as u64 {
                    Err(CommandError::from(format!("Attachment width of {} surpasses the maximum of {}.", attachment.width.unwrap(), self.max_width)))
                }
                
                else if attachment.height.unwrap() > self.max_height as u64 {
                    Err(CommandError::from(format!("Attachment height of {} surpasses the maximum of {}.", attachment.height.unwrap(), self.max_height)))
                }
                
                else {
                    Ok(attachment.download().await?)
                }
            },
            RawResult::Bytes(data) => {
                if data.len() > self.max_size {
                    Err(CommandError::from(format!(
                        "File is too big. (`{}` > `{}`)",
                        Self::_humanize_size(data.len() as f64),
                        Self::_humanize_size(self.max_size as f64),
                    )))
                }
                
                else {
                    Ok(data)
                }
            },
            RawResult::Url(mut url) => {
                url = url.trim_matches(|c| c == '<' || c == '>').to_string();
                
                if TENOR_REGEX.is_match(&url) {
                    url = self._scrape_tenor(url).await?;
                }
                
                else if GIPHY_REGEX.is_match(&url) {
                    url = self._scrape_giphy(url).await?;
                }
                
                let resp = reqwest::get(url).await?;

                if resp.status().is_success() {
                    let content_type = resp.headers().get("Content-Type").ok_or_else(|| CommandError::from("Invalid Content-Type."))?.to_str().unwrap();

                    if !allowed_content_types.contains(&content_type) {
                        return Err(CommandError::from(format!("Content-Type `{}` is not allowed", content_type)));
                    }

                    if let Some(content_length) = resp.headers().get("Content-Length") {
                        let size = u64::from_str_radix(content_length.to_str().unwrap(), 10_u32).unwrap_or(0_u64);

                        if size > self.max_size as u64 {
                            return Err(CommandError::from(
                                format!("File is too big. (`{}` > `{}`)",
                                    Self::_humanize_size(size as f64),
                                    Self::_humanize_size(self.max_size as f64),
                                )
                            ))
                        }

                        return Ok(resp.bytes().await?.to_vec());
                    }
                }

                Err(CommandError::from(format!("URL returned status code {}", resp.status())))
            }
        }
    }

    pub async fn resolve(&self, ctx: &Context, message: &Message, query: Option<String>) -> Result<Vec<u8>, CommandError> {
        let resolved_query = if query.is_some() && self.run_conversions {
            Some(
                Self::_run_conversions(ctx, message.guild_id, Some(message.channel_id), query.unwrap()).await
            )
        } else if query.is_some() {
            Some(Query::String(query.unwrap()))
        } else {
            None
        };

        let mut allowed_content_types = ALLOWED_CONTENT_TYPES.to_vec();
        let mut allowed_suffixes = ALLOWED_SUFFIXES.to_vec();

        if self.allow_gifs {
            allowed_content_types.push("image/gif");
            allowed_suffixes.push(".gif");
        }

        let fallback = async || {
            if let Some(a) = message.attachments.first() {
                return self._sanitize(RawResult::Attachment(a), &allowed_content_types, &allowed_suffixes).await
            }

            if let Some(reference) = &message.referenced_message {
                if let Some(a) = reference.attachments.first() {
                    return self._sanitize(RawResult::Attachment(a), &allowed_content_types, &allowed_suffixes).await
                }

                if let Some(embed) = reference.embeds.first() {
                    match embed.kind.as_str() {
                        "image" => if let Some(image) = &embed.thumbnail {
                            return self._sanitize(RawResult::Url(image.url.clone()), &allowed_content_types, &allowed_suffixes).await
                        },
                        "rich" => {
                            if let Some(image) = &embed.image {
                                return self._sanitize(RawResult::Url(image.url.to_string()), &allowed_content_types, &allowed_suffixes).await
                            }

                            if let Some(image) = &embed.thumbnail {
                                return self._sanitize(RawResult::Url(image.url.clone()), &allowed_content_types, &allowed_suffixes).await
                            }
                        },
                        _ => (),
                    }
                }

                if let Some(c) = URL_REGEX.captures_iter(&reference.content).next() {
                    if let Some(m) = c.get(1) {
                        return self._sanitize(RawResult::Url(m.as_str().to_string()), &allowed_content_types, &allowed_suffixes).await
                    }
                }
            }

            if self.allow_user_avatars && self.fallback_to_user_avatar {
                if let Some(avatar) = &message.author.avatar {
                    return self._sanitize(RawResult::Url(format!(
                        "https://cdn.discordapp.com/avatars/{}/{}.{}?size=512",
                        message.author.id,
                        avatar,
                        if self.allow_gifs && avatar.starts_with("a_") { "gif" } else { "png" }
                    )), &allowed_content_types, &allowed_suffixes).await
                }
            }

            Err(CommandError::from("Could not retrieve an image from the message."))
        };
        
        if let Some(q) = resolved_query {
            match q {
                Query::String(query) => {
                    
                }
            }
        }
        else {
            return fallback().await;
        }

        fallback().await
    }
}

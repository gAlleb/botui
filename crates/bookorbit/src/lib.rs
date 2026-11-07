//! Password sign-in, catalog navigation and reading individual files from BookOrbit.
pub mod bookmarks;
pub mod browse;
pub mod collections;
pub mod dashboard;
pub mod detail;
mod model;
pub mod progress;
pub mod reading_log;
mod session;
pub use session::Session;
#[cfg(test)]
mod tests;
mod transport;

pub use model::{Book, BookFile, BooksPage, Collection, Library};
use reqwest::{
    Method, StatusCode, Url,
    blocking::{Client, RequestBuilder, Response},
    header::{ACCEPT, CONTENT_TYPE},
    redirect::Policy,
};
use std::{error::Error, io::Write, time::Duration};
use transport::{HttpFailure, api_root, checked, decode, raw_bounded};

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub fn is_unauthorized(err: &(dyn Error + 'static)) -> bool {
    has_status(err, StatusCode::UNAUTHORIZED)
}

pub fn is_not_found(err: &(dyn Error + 'static)) -> bool {
    has_status(err, StatusCode::NOT_FOUND)
}

pub fn missing_route(err: &(dyn Error + 'static)) -> bool {
    has_status(err, StatusCode::NOT_FOUND) || has_status(err, StatusCode::METHOD_NOT_ALLOWED)
}

pub fn is_forbidden(err: &(dyn Error + 'static)) -> bool {
    has_status(err, StatusCode::FORBIDDEN)
}

fn has_status(err: &(dyn Error + 'static), status: StatusCode) -> bool {
    err.downcast_ref::<HttpFailure>()
        .is_some_and(|e| e.status == status)
}

#[derive(Clone, Copy)]
pub enum CatalogOrder {
    Title,
    Reading,
    Recent,
}

#[derive(Clone)]
pub struct BookOrbit {
    client: Client,
    root: Url,
}

impl BookOrbit {
    /// Check the configured installation URL before creating a client at sign-in.
    pub fn validate_address(server: &str) -> Result<()> {
        api_root(server).map(|_| ())
    }

    /// Accept an installation root, including its optional reverse proxy path.
    pub fn new(server: &str) -> Result<Self> {
        let root = api_root(server)?;
        let builder = Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(Policy::none());
        // Plain HTTP is restricted to a numeric loopback destination, even when
        // the host has configured a system-wide proxy.
        let builder = if root.scheme() == "http" {
            builder.no_proxy()
        } else {
            builder
        };
        Ok(Self {
            client: builder.build()?,
            root,
        })
    }

    pub fn libraries(&self, token: &str) -> Result<Vec<Library>> {
        read_json(self.authorized(Method::GET, "libraries", token)?)
    }

    pub fn collections(&self, token: &str) -> Result<Vec<Collection>> {
        read_json(self.authorized(Method::GET, "collections", token)?)
    }

    /// Query all books visible to this user, including titles outside collections.
    pub fn all_books(&self, token: &str, page: u32, size: u32) -> Result<BooksPage> {
        self.query_page(token, "books/query", page, size)
    }

    pub fn library_books(
        &self,
        token: &str,
        library_id: u64,
        page: u32,
        size: u32,
    ) -> Result<BooksPage> {
        self.query_page(token, &format!("libraries/{library_id}/books"), page, size)
    }

    fn query_page(&self, token: &str, path: &str, page: u32, size: u32) -> Result<BooksPage> {
        self.query_options(token, path, page, size, CatalogOrder::Title, None)
    }

    pub fn catalog_query(
        &self,
        token: &str,
        library: Option<u64>,
        page: u32,
        size: u32,
        order: CatalogOrder,
        search: Option<&str>,
    ) -> Result<BooksPage> {
        let path = library.map_or_else(
            || "books/query".into(),
            |id| format!("libraries/{id}/books"),
        );
        self.query_options(token, &path, page, size, order, search)
    }

    fn query_options(
        &self,
        token: &str,
        path: &str,
        page: u32,
        size: u32,
        order: CatalogOrder,
        search: Option<&str>,
    ) -> Result<BooksPage> {
        if !(1..=200).contains(&size) {
            return Err("Catalog page size must be between 1 and 200.".into());
        }
        let mut query = serde_json::json!({
            "pagination": {"page":page,"size":size},
            "sort": [match order {CatalogOrder::Title => serde_json::json!({"field":"title","dir":"asc"}), CatalogOrder::Reading => serde_json::json!({"field":"lastReadAt","dir":"desc"}), CatalogOrder::Recent => serde_json::json!({"field":"addedAt","dir":"desc"})}],
            "collapseSeries":false,
        });
        if let Some(q) = search {
            query["q"] = q.into();
        }
        if matches!(order, CatalogOrder::Reading) {
            query["filter"] = serde_json::json!({"type":"group","join":"AND","rules":[{"type":"rule","field":"readStatus","operator":"includesAny","value":["reading","rereading"]}]});
        }
        read_json(
            self.authorized(Method::POST, path, token)?
                .header(CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(&query)?),
        )
    }

    /// `page` starts at zero; the server accepts up to 100 entries per page.
    pub fn books(
        &self,
        token: &str,
        collection_id: u64,
        page: u32,
        size: u32,
    ) -> Result<BooksPage> {
        if !(1..=100).contains(&size) {
            return Err("Page size must be between 1 and 100.".into());
        }
        let path = format!(
            "collections/{collection_id}/books?page={page}&size={size}&collapseSeries=false"
        );
        read_json(self.authorized(Method::GET, &path, token)?)
    }

    /// Stream the original reader resource to a caller-owned staging file. Publish it
    /// only on success; a failed or interrupted transfer may already have written bytes.
    pub fn download(
        &self,
        token: &str,
        file_id: u64,
        writer: impl Write,
        limit: u64,
        cancel: &std::sync::atomic::AtomicBool,
        progress: &botui_transfer::Progress,
    ) -> Result<u64> {
        botui_transfer::check_cancel(cancel)?;
        self.edition(token, file_id, None)?
            .ok_or("Unexpected not-modified response")?
            .write_to(writer, limit, cancel, progress)
    }

    /// An authenticated 304 grants reuse of exactly the version the caller supplied.
    pub fn edition(
        &self,
        token: &str,
        file_id: u64,
        etag: Option<&str>,
    ) -> Result<Option<EditionBody>> {
        let mut request =
            self.authorized(Method::GET, &format!("books/files/{file_id}/serve"), token)?;
        if let Some(etag) = etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let response = request.send().map_err(|e| e.without_url())?;
        if etag.is_some() && response.status() == StatusCode::NOT_MODIFIED {
            return Ok(None);
        }
        let response = checked(response)?;
        if response.status() != StatusCode::OK {
            return Err("BookOrbit returned an incomplete file.".into());
        }
        if is_html(&response) {
            return Err(
                "BookOrbit returned HTML instead of a book. Check the address and sign-in.".into(),
            );
        }
        Ok(Some(EditionBody(response)))
    }

    pub fn thumbnail(&self, token: &str, book_id: u64) -> Result<Option<Vec<u8>>> {
        self.artwork(token, book_id, "thumbnail", 2 << 20)
    }

    pub fn cover(&self, token: &str, book_id: u64) -> Result<Option<Vec<u8>>> {
        self.artwork(token, book_id, "cover", 32 << 20)
    }

    fn artwork(
        &self,
        token: &str,
        book_id: u64,
        variant: &str,
        limit: u64,
    ) -> Result<Option<Vec<u8>>> {
        let path = format!("books/{book_id}/{variant}");
        let response = self
            .authorized(Method::GET, &path, token)?
            .header(ACCEPT, "image/*")
            .send()?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let response = checked(response)?;
        if is_html(&response) {
            return Err("BookOrbit returned HTML instead of cover art.".into());
        }
        Ok(Some(raw_bounded(response, limit)?))
    }

    fn authorized(&self, method: Method, path: &str, token: &str) -> Result<RequestBuilder> {
        if !valid_token(token) {
            return Err("Expected a temporary bearer token without whitespace.".into());
        }
        Ok(self
            .client
            .request(method, self.root.join(path)?)
            .bearer_auth(token))
    }
}

fn read_json<T: serde::de::DeserializeOwned>(request: RequestBuilder) -> Result<T> {
    let response = request
        .header(ACCEPT, "application/json")
        .send()
        .map_err(|err| err.without_url())?;
    decode(checked(response)?)
}

fn valid_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~+/=".contains(&b))
}

fn is_html(response: &Response) -> bool {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|v| {
            v.split(';')
                .next()
                .is_some_and(|t| t.trim().eq_ignore_ascii_case("text/html"))
        })
}

pub struct EditionBody(reqwest::blocking::Response);

impl EditionBody {
    pub fn etag(&self) -> Option<&str> {
        let tag = self.0.headers().get(reqwest::header::ETAG)?.to_str().ok()?;
        // An entity tag is a quoted opaque value, optionally prefixed with W/.
        let quoted = tag.strip_prefix("W/").unwrap_or(tag);
        (quoted.len() >= 2
            && quoted.starts_with('"')
            && quoted.ends_with('"')
            && !quoted[1..quoted.len() - 1].contains('"'))
        .then_some(tag)
    }

    pub fn write_to(
        self,
        writer: impl Write,
        limit: u64,
        cancel: &std::sync::atomic::AtomicBool,
        progress: &botui_transfer::Progress,
    ) -> Result<u64> {
        let length = self.0.content_length();
        Ok(botui_transfer::stream(
            self.0, writer, length, limit, cancel, progress,
        )?)
    }
}

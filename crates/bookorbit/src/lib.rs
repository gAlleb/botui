//! Password sign-in, catalog navigation and reading individual files from BookOrbit.
mod model;
pub mod progress;
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
use transport::{HttpFailure, api_root, checked, copy_bounded, decode, raw_bounded};

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub fn is_unauthorized(err: &(dyn Error + 'static)) -> bool {
    has_status(err, StatusCode::UNAUTHORIZED)
}

pub fn is_forbidden(err: &(dyn Error + 'static)) -> bool {
    has_status(err, StatusCode::FORBIDDEN)
}

fn has_status(err: &(dyn Error + 'static), status: StatusCode) -> bool {
    err.downcast_ref::<HttpFailure>()
        .is_some_and(|e| e.status == status)
}

#[derive(Clone)]
pub struct BookOrbit {
    client: Client,
    root: Url,
}

impl BookOrbit {
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
        if !(1..=200).contains(&size) {
            return Err("Размер страницы каталога должен быть от 1 до 200.".into());
        }
        let query = serde_json::json!({
            "pagination": {"page":page,"size":size},
            "sort": [{"field":"title","dir":"asc"}],
            "collapseSeries":false,
        });
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
            return Err("Размер страницы должен быть от 1 до 100.".into());
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
    ) -> Result<u64> {
        let path = format!("books/files/{file_id}/serve");
        let response = checked(
            self.authorized(Method::GET, &path, token)?
                .timeout(Duration::from_secs(600))
                .send()
                .map_err(|e| e.without_url())?,
        )?;
        if response.status() != StatusCode::OK {
            return Err("BookOrbit вернул неполный файл.".into());
        }
        if is_html(&response) {
            return Err(
                "BookOrbit вернул веб-страницу вместо файла книги. Проверьте адрес и вход.".into(),
            );
        }
        let length = response.content_length();
        if length.is_some_and(|n| n > limit) {
            return Err("Файл превышает установленное ограничение размера.".into());
        }
        let count = copy_bounded(response, writer, limit)?;
        if length.is_some_and(|n| n != count) || count == 0 {
            return Err("Файл получен не полностью.".into());
        }
        Ok(count)
    }

    pub fn thumbnail(&self, token: &str, book_id: u64) -> Result<Option<Vec<u8>>> {
        let path = format!("books/{book_id}/thumbnail");
        let response = self
            .authorized(Method::GET, &path, token)?
            .header(ACCEPT, "image/*")
            .send()?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let response = checked(response)?;
        if is_html(&response) {
            return Err("BookOrbit вернул веб-страницу вместо обложки.".into());
        }
        Ok(Some(raw_bounded(response, 2 * 1024 * 1024)?))
    }

    fn authorized(&self, method: Method, path: &str, token: &str) -> Result<RequestBuilder> {
        if !valid_token(token) {
            return Err("Требуется временный Bearer-токен без пробелов и переносов строки.".into());
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

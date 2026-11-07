use super::*;
use crate::transport::copy_bounded;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;

#[test]
fn address_accepts_secure_origin_or_loopback_and_preserves_installation_path() {
    assert_eq!(
        api_root("https://books.example/reader/").unwrap().path(),
        "/reader/api/v1/"
    );
    assert_eq!(
        api_root("http://localhost:8800").unwrap().host_str(),
        Some("127.0.0.1")
    );
    assert!(api_root("http://[::1]:8800").is_ok());
    for address in [
        "http://books.example",
        "http://192.168.1.12",
        "http://localhost.evil.test",
        "https://u:p@books.example",
        "https://books.example/?token=1",
        "file:///tmp/files",
    ] {
        assert!(api_root(address).is_err());
    }
}

#[test]
fn reads_collection_array_and_redacts_malformed_payloads() {
    let data = r#"[{"id":3,"userId":7,"name":"Моя полка","bookCount":42}]"#;
    assert_eq!(
        decode::<Vec<Collection>>(data.as_bytes()).unwrap(),
        vec![Collection {
            id: 3,
            name: "Моя полка".into(),
            book_count: 42
        }]
    );
    let error = decode::<Vec<Collection>>(
        br#"[{"id":"secret-token","bookCount":2,"name":"x"}]"#.as_slice(),
    )
    .unwrap_err();
    assert!(!error.to_string().contains("secret-token"));
}

#[test]
#[ignore = "requires a loopback TCP socket"]
fn loopback_get_and_redirect_rejection() {
    for (status, extra, body) in [
        ("200 OK", "", r#"[{"id":3,"name":"Полка","bookCount":10}]"#),
        (
            "302 Found",
            "Location: /do-not-follow\r\n",
            "sensitive external login body",
        ),
        ("401 Unauthorized", "", "do-not-log-this"),
    ] {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.local_addr().unwrap());
        let fixture = thread::spawn(move || {
            let (mut stream, _) = server.accept().unwrap();
            let mut lines = BufReader::new(&stream).lines();
            assert_eq!(
                lines.next().unwrap().unwrap(),
                "GET /api/v1/collections HTTP/1.1"
            );
            assert!(
                lines
                    .map(|l| l.unwrap())
                    .take_while(|l| !l.is_empty())
                    .any(|l| l.eq_ignore_ascii_case("authorization: Bearer mock-token"))
            );
            write!(
                stream,
                "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let result = BookOrbit::new(&url).unwrap().collections("mock-token");
        fixture.join().unwrap();
        match status {
            "200 OK" => assert_eq!(result.unwrap()[0].book_count, 10),
            _ => assert_eq!(
                result.unwrap_err().to_string(),
                format!("BookOrbit returned HTTP {}.", &status[..3])
            ),
        }
    }
}

#[test]
fn bounded_reader_keeps_unadvertised_downloads_inside_the_limit() {
    let mut bytes = Vec::new();
    assert!(copy_bounded(&b"abcdef"[..], &mut bytes, 5).is_err());
    assert_eq!(bytes, b"abcde");
    bytes.clear();
    assert_eq!(copy_bounded(&b"12345"[..], &mut bytes, 5).unwrap(), 5);
}

#[test]
fn cards_retain_all_file_choices_and_allow_missing_metadata() {
    let json = br#"{"items":[{"id":4,"title":null,"authors":[],"files":[
        {"id":11,"format":"epub","role":"primary","sizeBytes":32},
        {"id":12,"format":"fb2","role":"alternate","sizeBytes":null},
        {"id":13,"format":"pdf","role":"alternate","sizeBytes":35},
        {"id":14,"format":"djvu","role":"alternate","sizeBytes":41}]}],"page":0,"size":20,"total":57}"#;
    let page: BooksPage = decode(json.as_slice()).unwrap();
    assert_eq!((page.total, page.page, page.size), (57, 0, 20));
    assert!(page.items[0].title.is_none());
    assert_eq!(
        page.items[0].files.iter().map(|f| f.id).collect::<Vec<_>>(),
        [11, 12, 13, 14]
    );
}

fn fixture_request(listener: &TcpListener) -> (std::net::TcpStream, String, Vec<u8>) {
    let (stream, _) = listener.accept().unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut reader = BufReader::new(&stream);
    let mut request = String::new();
    let mut size = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("Content-Length")
        {
            size = value.trim().parse().unwrap();
        }
        request.push_str(&line);
    }
    let mut payload = vec![0; size];
    std::io::Read::read_exact(&mut reader, &mut payload).unwrap();
    (stream, request, payload)
}

fn fixture_reply(stream: &mut std::net::TcpStream, status: &str, mime: &str, body: &[u8]) {
    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
    stream.write_all(body).unwrap();
}

// Return the request headers and request body as observed by a loopback-only server.
fn exchange<T>(
    status: &str,
    mime: &str,
    body: &[u8],
    call: impl FnOnce(&BookOrbit) -> Result<T>,
) -> (Result<T>, String, Vec<u8>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}/shelf", listener.local_addr().unwrap());
    thread::scope(|scope| {
        let worker = scope.spawn(move || {
            let (mut stream, request, payload) = fixture_request(&listener);
            fixture_reply(&mut stream, status, mime, body);
            (request, payload)
        });
        let result = call(&BookOrbit::new(&origin).unwrap());
        let (request, payload) = worker.join().unwrap();
        (result, request, payload)
    })
}

#[test]
#[ignore = "requires a loopback TCP socket"]
fn native_session_catalog_and_selected_file_routes() {
    let (login, request, payload) = exchange(
        "200 OK",
        "application/json",
        br#"{"accessToken":"session.mock","refreshToken":"never-used"}"#,
        |server| server.sign_in("test-user", "пример-пароля"),
    );
    assert!(request.starts_with("POST /shelf/api/v1/auth/login HTTP/1.1\r\n"));
    let credentials: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    assert_eq!(
        credentials,
        serde_json::json!({"username":"test-user","password":"пример-пароля","clientKind":"native","deviceLabel":"botui"})
    );
    let token = login.unwrap().with_token(|_, s| Ok(s.to_owned())).unwrap();
    let (page, req, _) = exchange(
        "200 OK",
        "application/json",
        br#"{"items":[],"page":2,"size":9,"total":18}"#,
        |s| s.books(&token, 7, 2, 9),
    );
    assert!(req.starts_with(
        "GET /shelf/api/v1/collections/7/books?page=2&size=9&collapseSeries=false HTTP/1.1\r\n"
    ));
    assert!(
        req.to_ascii_lowercase()
            .contains("authorization: bearer session.mock\r\n")
    );
    assert_eq!(page.unwrap().total, 18);

    let mut dest = Vec::new();
    let (download, req, _) = exchange("200 OK", "application/pdf", b"%PDF-data", |s| {
        s.download(
            &token,
            82,
            &mut dest,
            50,
            &std::sync::atomic::AtomicBool::new(false),
            &botui_transfer::Progress::default(),
        )
    });
    assert!(req.starts_with("GET /shelf/api/v1/books/files/82/serve HTTP/1.1\r\n"));
    assert_eq!(download.unwrap(), 9);
    assert_eq!(dest, b"%PDF-data");
    dest.clear();
    let (oversize, _, _) = exchange("200 OK", "application/pdf", b"12345", |s| {
        s.download(
            &token,
            82,
            &mut dest,
            2,
            &std::sync::atomic::AtomicBool::new(false),
            &botui_transfer::Progress::default(),
        )
    });
    assert!(oversize.is_err());
    assert!(dest.is_empty());
    let (thumb, req, _) = exchange("404 Not Found", "application/json", b"{}", |s| {
        s.thumbnail(&token, 7)
    });
    assert!(req.starts_with("GET /shelf/api/v1/books/7/thumbnail HTTP/1.1\r\n"));
    assert_eq!(thumb.unwrap(), None);
}

#[test]
#[ignore = "requires a loopback TCP socket"]
fn explains_expired_credentials_and_oidc_without_echoing_remote_error_text() {
    let (disabled, _, _) = exchange(
        "403 Forbidden",
        "application/json",
        br#"{"errorCode":"password_auth_disabled","message":"confidential remote message"}"#,
        |s| s.sign_in("user", "password").map(|_| ()),
    );
    let message = disabled.unwrap_err().to_string();
    assert!(message.contains("OIDC"));
    assert!(!message.contains("confidential"));
    let (expired, _, _) = exchange("401 Unauthorized", "application/json", b"{}", |s| {
        s.books("expired", 1, 0, 2)
    });
    assert!(is_unauthorized(expired.unwrap_err().as_ref()));
    let mut dest = Vec::new();
    let (proxy, _, _) = exchange("200 OK", "text/html; charset=utf-8", b"<html>SSO", |s| {
        s.download(
            "token",
            2,
            &mut dest,
            100,
            &std::sync::atomic::AtomicBool::new(false),
            &botui_transfer::Progress::default(),
        )
    });
    assert!(proxy.is_err());
    assert!(dest.is_empty());
}

fn unauthorized() -> Box<dyn std::error::Error + Send + Sync> {
    HttpFailure {
        status: StatusCode::UNAUTHORIZED,
        description: None,
    }
    .into()
}

#[test]
#[ignore = "requires a loopback TCP socket"]
fn concurrent_readers_rotate_once_then_logout_revokes_the_latest_token() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = BookOrbit::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let c = "c".repeat(64);
        for (route, expected, send) in [
            (
                "login",
                None,
                serde_json::json!({"accessToken":"same.jwt","refreshToken":a}),
            ),
            (
                "refresh",
                Some(&a),
                serde_json::json!({"accessToken":"same.jwt","refreshToken":b}),
            ),
            (
                "refresh",
                Some(&b),
                serde_json::json!({"accessToken":"new.jwt","refreshToken":c}),
            ),
            ("logout", Some(&c), serde_json::json!({})),
        ] {
            let (mut stream, headers, payload) = fixture_request(&listener);
            assert!(headers.starts_with(&format!("POST /api/v1/auth/{route} HTTP/1.1\r\n")));
            if let Some(value) = expected {
                let payload: serde_json::Value = serde_json::from_slice(&payload).unwrap();
                assert_eq!(payload, serde_json::json!({"refreshToken":value}));
            }
            fixture_reply(
                &mut stream,
                "200 OK",
                "application/json",
                &serde_json::to_vec(&send).unwrap(),
            );
        }
    });
    let session = api.sign_in("user", "password").unwrap();
    let barrier = std::sync::Barrier::new(2);
    thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                let mut tries = 0;
                session
                    .with_token(|_, access| {
                        tries += 1;
                        if tries == 1 {
                            barrier.wait();
                            return Err(unauthorized());
                        }
                        assert_eq!(access, "same.jwt");
                        Ok(())
                    })
                    .unwrap();
                assert_eq!(tries, 2);
            });
        }
    });
    session
        .with_token(|_, token| {
            if token == "new.jwt" {
                Ok(())
            } else {
                Err(unauthorized())
            }
        })
        .unwrap();
    session.logout().unwrap();
    assert!(is_unauthorized(
        session
            .with_token::<()>(|_, _| panic!("closed session ran a request"))
            .unwrap_err()
            .as_ref()
    ));
    server.join().unwrap();
}

#[test]
#[ignore = "requires a loopback TCP socket"]
fn missing_or_revoked_refresh_returns_401_without_retrying_requests() {
    let (session, _, _) = exchange(
        "200 OK",
        "application/json",
        br#"{"accessToken":"legacy"}"#,
        |api| api.sign_in("user", "password"),
    );
    let session = session.unwrap();
    let mut calls = 0;
    let err = session
        .with_token::<()>(|_, _| {
            calls += 1;
            Err(unauthorized())
        })
        .unwrap_err();
    assert_eq!(calls, 1);
    assert!(is_unauthorized(err.as_ref()));

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = BookOrbit::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _, _) = fixture_request(&listener);
        let credentials = serde_json::json!({"accessToken":"first","refreshToken":"a".repeat(64)});
        fixture_reply(
            &mut stream,
            "200 OK",
            "application/json",
            &serde_json::to_vec(&credentials).unwrap(),
        );
        let (mut stream, request, _) = fixture_request(&listener);
        assert!(request.starts_with("POST /api/v1/auth/refresh "));
        fixture_reply(&mut stream, "401 Unauthorized", "application/json", b"{}");
    });
    let session = api.sign_in("user", "password").unwrap();
    let err = session
        .with_token::<()>(|_, _| Err(unauthorized()))
        .unwrap_err();
    assert!(is_unauthorized(err.as_ref()));
    assert!(is_unauthorized(
        session
            .with_token::<()>(|_, _| panic!("revoked session sent a request"))
            .unwrap_err()
            .as_ref()
    ));
    server.join().unwrap();
}

#[test]
#[ignore = "requires a loopback TCP socket"]
fn global_catalog_and_library_scopes_use_their_own_routes_without_collections() {
    let (response, headers, _) = exchange(
        "200 OK",
        "application/json",
        br#"[{"id":10,"name":"Main","bookCount":27,"folders":[]},{"id":21,"name":"Shared"}]"#,
        |api| api.libraries("native-token"),
    );
    let libraries = response.unwrap();
    assert!(headers.starts_with("GET /shelf/api/v1/libraries HTTP/1.1\r\n"));
    assert_eq!(libraries[0].book_count, Some(27));
    assert_eq!(libraries[1].book_count, None);

    for (library, endpoint) in [(None, "books/query"), (Some(21), "libraries/21/books")] {
        let (response, headers, body) = exchange("201 Created","application/json",
            br#"{"page":1,"size":10,"total":11,"items":[{"id":42,"title":"Classic","authors":[],"files":[{"id":5,"format":"pdf","role":"primary","sizeBytes":200}]}]}"#,
            |api| match library {
                None => api.all_books("native-token",1,10),
                Some(id) => api.library_books("native-token",id,1,10),
            });
        assert!(headers.starts_with(&format!("POST /shelf/api/v1/{endpoint} HTTP/1.1\r\n")));
        let headers = headers.to_ascii_lowercase();
        assert!(headers.contains("authorization: bearer native-token\r\n"));
        assert!(headers.contains("content-type: application/json\r\n"));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({
                "pagination":{"page":1,"size":10},"sort":[{"field":"title","dir":"asc"}],"collapseSeries":false
            })
        );
        assert_eq!(
            response.unwrap().items[0].files[0].format.as_deref(),
            Some("pdf")
        );
    }
    let (denied, _, _) = exchange("403 Forbidden", "application/json", b"{}", |api| {
        api.library_books("native-token", 33, 0, 10)
    });
    let error = denied.unwrap_err();
    assert!(is_forbidden(error.as_ref()));
    assert!(!is_unauthorized(error.as_ref()));
}

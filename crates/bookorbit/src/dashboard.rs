//! Read the account dashboard, including older servers that return a bare shelf array.
use crate::{Book, BookOrbit, Method, Result, read_json};
use serde::Deserialize;

#[derive(Clone, Copy)]
pub enum DashboardShelf {
    Continue,
    Recent,
    Discover,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ShelfResponse {
    Window { books: Vec<Book> },
    Books(Vec<Book>),
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Widget {
    #[serde(rename = "reading-streak", rename_all = "camelCase")]
    Streak {
        current_streak: u32,
        longest_streak: u32,
    },
    #[serde(rename = "reading-goal", rename_all = "camelCase")]
    Goal {
        goal_books: Option<u32>,
        completed_books: u32,
        year: u32,
    },
    #[serde(rename = "monthly-challenge")]
    Challenge {
        title: String,
        progress: u32,
        target: u32,
    },
    #[serde(rename = "reading-dna", rename_all = "camelCase")]
    Dna {
        archetype: String,
        books_analyzed: u32,
    },
}
impl BookOrbit {
    pub fn dashboard_shelf(
        &self,
        token: &str,
        shelf: DashboardShelf,
        limit: u32,
    ) -> Result<Vec<Book>> {
        let key = match shelf {
            DashboardShelf::Continue => "continue-reading",
            DashboardShelf::Recent => "recently-added",
            DashboardShelf::Discover => "random",
        };
        let request = self.authorized(
            Method::GET,
            &format!("dashboard/scrollers/{key}?limit={limit}"),
            token,
        )?;
        Ok(match read_json(request)? {
            ShelfResponse::Window { books } | ShelfResponse::Books(books) => books,
        })
    }
    pub fn dashboard_widgets(&self, token: &str) -> Result<Vec<Widget>> {
        #[derive(Deserialize)]
        struct Batch {
            items: Vec<serde_json::Value>,
        }
        let names = [
            "reading-streak",
            "reading-goal",
            "monthly-challenge",
            "reading-dna",
        ];
        let body = serde_json::json!({"widgets": names});
        let request = self
            .authorized(Method::POST, "dashboard/widgets/batch", token)?
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(&body)?);
        let batch: Batch = match read_json(request) {
            Ok(batch) => batch,
            Err(e) if crate::missing_route(e.as_ref()) => {
                let mut items = Vec::new();
                for name in names {
                    let request =
                        self.authorized(Method::GET, &format!("dashboard/widgets/{name}"), token)?;
                    match read_json::<serde_json::Value>(request) {
                        Ok(data) => items.push(serde_json::json!({"type":name,"data":data})),
                        Err(e) if crate::missing_route(e.as_ref()) => {}
                        Err(e) => return Err(e),
                    }
                }
                Batch { items }
            }
            Err(e) => return Err(e),
        };
        Ok(batch
            .items
            .into_iter()
            .filter(|item| item["failed"] != true)
            .filter_map(|item| serde_json::from_value(item).ok())
            .collect())
    }
}

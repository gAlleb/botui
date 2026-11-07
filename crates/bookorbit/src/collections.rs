use crate::{BookOrbit, Collection, Result, checked, read_json};
use reqwest::Method;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Membership {
    pub id: u64,
    pub name: String,
    pub member_count: u64,
}

impl BookOrbit {
    pub fn memberships(&self, token: &str, book: u64) -> Result<Vec<Membership>> {
        read_json(self.authorized(Method::GET, &format!("collections?bookIds={book}"), token)?)
    }

    pub fn create_collection(&self, token: &str, name: &str) -> Result<Collection> {
        let body = serde_json::json!({"name":name, "icon":"FolderOpen", "isPublic":false});
        read_json(
            self.authorized(Method::POST, "collections", token)?
                .header("Content-Type", "application/json")
                .body(serde_json::to_vec(&body)?),
        )
    }

    pub fn delete_collection(&self, token: &str, id: u64) -> Result<()> {
        let response = self
            .authorized(Method::DELETE, &format!("collections/{id}"), token)?
            .send()
            .map_err(|e| e.without_url())?;
        checked(response)?;
        Ok(())
    }

    pub fn set_membership(&self, token: &str, collection: u64, book: u64, add: bool) -> Result<()> {
        let method = if add { Method::POST } else { Method::DELETE };
        checked(
            self.authorized(method, &format!("collections/{collection}/books"), token)?
                .header("Content-Type", "application/json")
                .body(serde_json::to_vec(&serde_json::json!({"bookIds":[book]}))?)
                .send()
                .map_err(|e| e.without_url())?,
        )?;
        Ok(())
    }
}

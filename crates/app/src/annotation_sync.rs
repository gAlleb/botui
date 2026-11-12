//! Annotations sync independently from reading progress and never block its input loop.
mod engine;
mod mapping;
#[cfg(test)]
mod tests;
use botui_bookorbit::{
    Session,
    annotations::{Annotation, New},
};
use botui_store::{LocalState, Note};
use engine::Remote;
use mapping::Mapper;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Context {
    pub session: Arc<Session>,
    pub account: String,
    pub book: u64,
    pub file: u64,
}
impl Context {
    fn scope(&self) -> String {
        format!("{}-{}-{}", self.account, self.book, self.file)
    }
}
impl Remote for Context {
    fn list(&self) -> botui_bookorbit::Result<Vec<Annotation>> {
        self.session
            .with_token(|a, t| a.annotations(t, self.book, self.file))
    }
    fn create(&self, new: &New) -> botui_bookorbit::Result<Annotation> {
        self.session
            .with_token(|a, t| a.add_annotation(t, self.book, new))
    }
    fn edit(&self, id: u64, note: &str, color: &str) -> botui_bookorbit::Result<Annotation> {
        self.session
            .with_token(|a, t| a.edit_annotation(t, self.book, id, note, color))
    }
    fn delete(&self, id: u64) -> botui_bookorbit::Result<()> {
        self.session
            .with_token(|a, t| a.remove_annotation(t, self.book, id))
    }
}
type Job = JoinHandle<(Mapper, Result<String, String>)>;
pub struct Live {
    context: Context,
    path: PathBuf,
    mapper: Option<Mapper>,
    job: Option<Job>,
    next: Instant,
    remote_at: Instant,
    observed: Vec<Note>,
    pub message: String,
}
impl Live {
    pub fn text(context: Context, path: &Path, doc: Arc<botui_core::Document>) -> Self {
        Self::new(context, path, Mapper::Text(doc))
    }
    pub fn pdf(context: Context, path: &Path) -> Self {
        Self::new(
            context,
            path,
            Mapper::Pdf {
                path: path.into(),
                pages: Default::default(),
            },
        )
    }
    fn new(context: Context, path: &Path, mapper: Mapper) -> Self {
        Self {
            context,
            path: path.into(),
            mapper: Some(mapper),
            job: None,
            next: Instant::now(),
            remote_at: Instant::now(),
            observed: vec![],
            message: "BookOrbit annotations: connecting…".into(),
        }
    }
    pub fn timeout(&self) -> Duration {
        if self.job.is_some() {
            Duration::from_millis(150)
        } else {
            self.next.saturating_duration_since(Instant::now())
        }
    }
    fn collect(&mut self) -> bool {
        let Some(job) = self.job.take() else {
            return false;
        };
        match job.join() {
            Ok((mapper, result)) => {
                self.mapper = Some(mapper);
                self.message =
                    result.unwrap_or_else(|e| format!("Annotations pending locally: {e}"));
            }
            Err(_) => {
                self.message = "Annotation worker interrupted; reopen this book to retry".into()
            }
        }
        self.next = Instant::now() + Duration::from_secs(2);
        self.remote_at = Instant::now() + Duration::from_secs(15);
        true
    }
    pub fn tick(&mut self) -> bool {
        if self.job.as_ref().is_some_and(|j| j.is_finished()) {
            return self.collect();
        }
        if self.job.is_some() || Instant::now() < self.next {
            return false;
        }
        self.next = Instant::now() + Duration::from_secs(2);
        let notes = LocalState::for_file(&self.path).and_then(|s| s.notes());
        match notes {
            Ok(notes) if notes != self.observed || Instant::now() >= self.remote_at => {
                self.observed = notes;
                self.start(false);
            }
            Err(e) => self.message = format!("Cannot read annotations: {e}"),
            _ => (),
        }
        false
    }
    fn start(&mut self, retry: bool) {
        let Some(mut mapper) = self.mapper.take() else {
            return;
        };
        let context = self.context.clone();
        let path = self.path.clone();
        self.job = Some(thread::spawn(move || {
            let result = (|| -> botui_bookorbit::Result<String> {
                let store = LocalState::for_file(&path)?;
                Ok(engine::sync(
                    &store,
                    &context.scope(),
                    context.file,
                    &mut mapper,
                    &context,
                    retry,
                )?
                .message())
            })()
            .map_err(|e| e.to_string());
            (mapper, result)
        }));
    }
    pub fn review(&mut self, term: &mut crate::AppTerminal) -> std::io::Result<()> {
        if self.job.as_ref().is_some_and(|j| j.is_finished()) {
            self.collect();
        }
        let labels = vec![
            self.message.clone(),
            "Sync now / retry unconfirmed uploads".into(),
            "Close".into(),
        ];
        if crate::prompt::choose(term, "BookOrbit annotations", &labels, 2)? == Some(1) {
            // An explicit retry can resend a missing POST; always reconcile its journal first.
            if self.job.is_some() {
                self.collect();
            }
            self.start(true);
        }
        Ok(())
    }
    pub fn finish(&mut self, notices: &mut Vec<String>) {
        if self.job.is_some() {
            self.collect();
        }
        if LocalState::for_file(&self.path)
            .and_then(|s| s.notes())
            .is_ok_and(|n| n != self.observed)
        {
            self.start(false);
            self.collect();
        }
        if !self
            .message
            .starts_with("BookOrbit annotations synchronized")
            || self.message.contains(" · ")
        {
            notices.push(self.message.clone());
        }
    }
}

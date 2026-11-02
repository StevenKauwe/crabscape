//! The worker is next to the current build, including preview builds whose
//! page URL may or may not end in a slash.

pub fn worker_path(path: &str) -> String {
    let directory = if path.ends_with('/') {
        path.to_owned()
    } else if path
        .rsplit('/')
        .next()
        .is_some_and(|last| last.contains('.'))
    {
        path.rsplit_once('/')
            .map_or_else(|| "/".to_owned(), |(parent, _)| format!("{parent}/"))
    } else {
        format!("{path}/")
    };
    format!("{directory}cellworker/worker.js")
}

#[cfg(test)]
mod tests {
    use super::worker_path;

    #[test]
    fn roots_and_preview_pages_stay_inside_their_build() {
        assert_eq!(worker_path("/"), "/cellworker/worker.js");
        assert_eq!(worker_path("/crabscape"), "/crabscape/cellworker/worker.js");
        assert_eq!(
            worker_path("/crabscape/"),
            "/crabscape/cellworker/worker.js"
        );
        assert_eq!(
            worker_path("/crabscape/pr-352"),
            "/crabscape/pr-352/cellworker/worker.js"
        );
        assert_eq!(
            worker_path("/crabscape/pr-352/"),
            "/crabscape/pr-352/cellworker/worker.js"
        );
        assert_eq!(
            worker_path("/crabscape/pr-352/index.html"),
            "/crabscape/pr-352/cellworker/worker.js"
        );
    }
}

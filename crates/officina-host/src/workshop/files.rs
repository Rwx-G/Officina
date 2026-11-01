//! The catalogue files of the workshop (AD-101): each workshop project's
//! Dockerfile and its folder, and the CA file, read for phase 2 within
//! their bounds. A file over its bound is never read partially: it is
//! [`FileContent::TooLarge`], an issue. The window, doctor and `officina
//! relay --json` read them; `officina run` and the hook's gate never do
//! (D-244).

use std::fs;
use std::io::Read;
use std::path::Path;

use officina_core::floor::FloorFile;
use officina_core::workshop::floor::{
    CatalogueFiles, ContextFile, ContextProblem, DockerfileFiles, FileContent, MAX_CA_BYTES,
    MAX_CONTEXT_BYTES, MAX_CONTEXT_FILES, MAX_DOCKERFILE_BYTES, file_requests,
};

/// Reads what `floor`, read from `floor_path`, asks phase 2 to read.
pub fn read(floor: &FloorFile, floor_path: &str) -> CatalogueFiles {
    let requests = file_requests(floor, floor_path);
    CatalogueFiles {
        read: true,
        ca_file: requests
            .ca_file
            .map(|path| bounded(Path::new(&path), MAX_CA_BYTES)),
        dockerfiles: requests
            .dockerfiles
            .into_iter()
            .map(|request| {
                let path = Path::new(&request.path);
                let files = DockerfileFiles {
                    dockerfile: bounded(path, MAX_DOCKERFILE_BYTES),
                    context: context(path.parent().unwrap_or_else(|| Path::new("."))),
                };
                (request.project, files)
            })
            .collect(),
    }
}

/// The file at `path`, at most `limit` bytes: one more byte read means
/// [`FileContent::TooLarge`]. A link or anything but a regular file is
/// unreadable, so a Dockerfile cannot point outside its project.
fn bounded(path: &Path, limit: usize) -> FileContent {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => return FileContent::Unreadable("not a regular file".to_owned()),
        Err(e) => return FileContent::Unreadable(e.to_string()),
    }
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) => return FileContent::Unreadable(e.to_string()),
    };
    let mut bytes = Vec::new();
    let cap = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    match file.take(cap).read_to_end(&mut bytes) {
        Ok(_) if bytes.len() > limit => FileContent::TooLarge,
        Ok(_) => FileContent::Bytes(bytes),
        Err(e) => FileContent::Unreadable(e.to_string()),
    }
}

/// The build context: every entry of `dir`, regular files only, at most
/// [`MAX_CONTEXT_FILES`] and [`MAX_CONTEXT_BYTES`] in all.
fn context(dir: &Path) -> Result<Vec<ContextFile>, ContextProblem> {
    let entries = fs::read_dir(dir).map_err(|e| ContextProblem::Unreadable(e.to_string()))?;
    let mut files = Vec::new();
    let mut total = 0usize;
    for entry in entries {
        let entry = entry.map_err(|e| ContextProblem::Unreadable(e.to_string()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry
            .file_type()
            .map_err(|e| ContextProblem::Unreadable(e.to_string()))?;
        if kind.is_symlink() {
            return Err(ContextProblem::Symlink(name));
        }
        if kind.is_dir() {
            return Err(ContextProblem::Subfolder(name));
        }
        if files.len() == MAX_CONTEXT_FILES {
            return Err(ContextProblem::TooManyFiles);
        }
        let left = MAX_CONTEXT_BYTES.saturating_sub(total);
        let bytes = match bounded(&entry.path(), left) {
            FileContent::Bytes(bytes) => bytes,
            FileContent::TooLarge => return Err(ContextProblem::TooLarge),
            FileContent::Unreadable(why) => return Err(ContextProblem::Unreadable(why)),
        };
        total += bytes.len();
        files.push(ContextFile { name, bytes });
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use officina_core::floor::parse_floor;

    const DOCKERFILE: &str = "FROM officina-workshop:local\nRUN true\n";

    fn floor_text(ca: &Path) -> String {
        format!(
            "schema = 2\n[workshop]\nca_file = '{}'\n[[project]]\nname = \"api\"\npath = \"Api\"\n[project.workshop]\ndockerfile = \"tools/Dockerfile\"\n",
            ca.display()
        )
    }

    struct Floor {
        dir: tempfile::TempDir,
    }

    impl Floor {
        fn new() -> Floor {
            let dir = tempfile::tempdir().unwrap();
            fs::create_dir_all(dir.path().join("Api").join("tools")).unwrap();
            fs::write(dir.path().join("Api/tools/Dockerfile"), DOCKERFILE).unwrap();
            fs::write(
                dir.path().join("ca.pem"),
                "-----BEGIN CERTIFICATE-----\nTUlJQmV4YW1wbGU=\n-----END CERTIFICATE-----\n",
            )
            .unwrap();
            Floor { dir }
        }

        fn read(&self) -> CatalogueFiles {
            let floor_path = self.dir.path().join("officina.toml");
            let text = floor_text(&self.dir.path().join("ca.pem"));
            fs::write(&floor_path, &text).unwrap();
            read(&parse_floor(&text).unwrap(), &floor_path.to_string_lossy())
        }

        fn tools(&self) -> std::path::PathBuf {
            self.dir.path().join("Api").join("tools")
        }
    }

    #[test]
    fn reads_within_bounds() {
        let floor = Floor::new();
        fs::write(floor.tools().join("requirements.txt"), "ruff\n").unwrap();
        let files = floor.read();
        assert!(files.read);
        assert!(matches!(files.ca_file, Some(FileContent::Bytes(_))));
        let api = &files.dockerfiles["api"];
        assert_eq!(
            api.dockerfile,
            FileContent::Bytes(DOCKERFILE.as_bytes().to_vec())
        );
        let names: Vec<&str> = api
            .context
            .as_ref()
            .unwrap()
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(names, ["Dockerfile", "requirements.txt"]);
        let exact = floor.tools().join("exact.bin");
        fs::write(&exact, vec![b'x'; 10]).unwrap();
        assert_eq!(bounded(&exact, 10), FileContent::Bytes(vec![b'x'; 10]));
    }

    #[test]
    fn an_oversized_dockerfile_is_an_issue_not_a_truncation() {
        let floor = Floor::new();
        fs::write(
            floor.tools().join("Dockerfile"),
            format!("{DOCKERFILE}{}", "#".repeat(MAX_DOCKERFILE_BYTES)),
        )
        .unwrap();
        let files = floor.read();
        assert_eq!(files.dockerfiles["api"].dockerfile, FileContent::TooLarge);
        fs::write(
            floor.dir.path().join("ca.pem"),
            vec![b'x'; MAX_CA_BYTES + 1],
        )
        .unwrap();
        assert_eq!(floor.read().ca_file, Some(FileContent::TooLarge));
        fs::write(floor.tools().join("Dockerfile"), DOCKERFILE).unwrap();
        for i in 0..4 {
            fs::write(
                floor.tools().join(format!("big{i}.bin")),
                vec![b'x'; MAX_CONTEXT_BYTES / 4],
            )
            .unwrap();
        }
        assert_eq!(
            floor.read().dockerfiles["api"].context,
            Err(ContextProblem::TooLarge)
        );
    }

    #[test]
    fn a_subfolder_or_symlink_in_the_context_is_an_issue() {
        let floor = Floor::new();
        fs::create_dir(floor.tools().join("sub")).unwrap();
        assert_eq!(
            floor.read().dockerfiles["api"].context,
            Err(ContextProblem::Subfolder("sub".to_owned()))
        );
        fs::remove_dir(floor.tools().join("sub")).unwrap();
        for i in 0..MAX_CONTEXT_FILES {
            fs::write(floor.tools().join(format!("f{i}")), "x").unwrap();
        }
        assert_eq!(
            floor.read().dockerfiles["api"].context,
            Err(ContextProblem::TooManyFiles)
        );
    }

    #[test]
    fn a_missing_file_is_unreadable() {
        let floor = Floor::new();
        fs::remove_file(floor.tools().join("Dockerfile")).unwrap();
        fs::remove_file(floor.dir.path().join("ca.pem")).unwrap();
        let files = floor.read();
        assert!(matches!(
            files.dockerfiles["api"].dockerfile,
            FileContent::Unreadable(_)
        ));
        assert!(matches!(files.ca_file, Some(FileContent::Unreadable(_))));
    }
}

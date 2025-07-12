use futures_util::future::try_join_all;
use jwalk::WalkDir;
use tokio::{fs, io};

use std::{
    fmt,
    path::{Path, PathBuf},
};

pub struct Asset {
    path: PathBuf,
    content: String,
}

pub struct AssetList(pub Vec<Asset>);

pub async fn read_all_files_in_dir(dir: impl AsRef<Path>) -> io::Result<AssetList> {
    let mut tasks = Vec::new();

    for entry in WalkDir::new(&dir) {
        let entry = entry.map_err(io::Error::other)?;
        let path = entry.path().to_path_buf();

        // Skip if not a file
        if path.is_file() {
            let task = async move {
                let content = fs::read_to_string(&path).await?;
                Ok::<Asset, io::Error>(Asset { path, content })
            };
            tasks.push(task);
        }
    }

    let assets = try_join_all(tasks).await?;
    Ok(AssetList(assets))
}
impl fmt::Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.path.display())?;
        writeln!(f, "```")?;
        writeln!(f, "{}", self.content)?;
        writeln!(f, "```")
    }
}

impl fmt::Display for AssetList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for asset in &self.0 {
            writeln!(f, "{}\n", asset)?;
        }
        Ok(())
    }
}

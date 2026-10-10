//! 输出路径冲突检查及跨进程写入锁。
use eyre::{Result, bail};
use std::{
    fs::{self, File, OpenOptions},
    path::{Component, Path, PathBuf},
};

/// 逐级解析符号链接后再处理 `..`；允许末端文件尚未创建。
fn identity(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut result = PathBuf::new();
    for part in absolute.components() {
        match part {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => {
                result.push(other.as_os_str());
                match fs::symlink_metadata(&result) {
                    Ok(_) => result = fs::canonicalize(&result)?,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }
    Ok(result)
}

/// 所有写入目标必须互异，且不能覆盖任何输入文件。
pub fn validate(outputs: &[PathBuf], inputs: &[PathBuf]) -> Result<()> {
    let inputs = inputs
        .iter()
        .map(|p| identity(p))
        .collect::<Result<std::collections::HashSet<_>>>()?;
    let mut seen = std::collections::HashSet::new();
    for output in outputs {
        let resolved = identity(output)?;
        if !seen.insert(resolved.clone()) {
            bail!("输出路径冲突: {}", output.display());
        }
        if resolved.is_dir() {
            bail!("输出路径是目录: {}", output.display());
        }
        if inputs.contains(&resolved) {
            bail!("输出不能覆盖输入文件: {}", output.display());
        }
    }
    Ok(())
}

pub fn previous_input(previous: Option<&Path>) -> Option<PathBuf> {
    previous
        .filter(|p| *p != Path::new("clickhouse"))
        .map(Path::to_path_buf)
}

/// 锁文件保留在临时目录，避免删除锁文件后另一进程锁住不同 inode。
pub struct OutputLocks(Vec<File>);

impl OutputLocks {
    pub fn acquire(outputs: &[PathBuf]) -> Result<Self> {
        let dir = std::env::temp_dir().join("rustdx_output_locks");
        fs::create_dir_all(&dir)?;
        let mut locks = Self(Vec::new());
        for output in outputs {
            let key = rustdx_complete::cache::stable_key(&identity(output)?.to_string_lossy());
            let file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(dir.join(key))?;
            file.try_lock()
                .map_err(|e| eyre::eyre!("输出已被其他任务使用 {}: {e}", output.display()))?;
            locks.0.push(file);
        }
        Ok(locks)
    }
}

impl Drop for OutputLocks {
    fn drop(&mut self) {
        for file in &self.0 {
            let _ = file.unlock();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_and_concurrent_writers_are_rejected() {
        let dir = std::env::temp_dir().join(format!("rustdx-paths-{}", std::process::id()));
        fs::create_dir_all(dir.join("sub")).unwrap();
        let input = dir.join("input.csv");
        fs::write(&input, b"keep").unwrap();
        assert!(
            validate(
                &[dir.join("sub/../input.csv")],
                std::slice::from_ref(&input)
            )
            .is_err()
        );
        let output = dir.join("new.csv");
        assert!(validate(&[output.clone(), dir.join("./new.csv")], &[]).is_err());
        #[cfg(unix)]
        {
            let link = dir.join("alias");
            std::os::unix::fs::symlink(dir.join("sub"), &link).unwrap();
            assert!(validate(&[link.join("../input.csv")], std::slice::from_ref(&input)).is_err());
        }
        let lock = OutputLocks::acquire(std::slice::from_ref(&output)).unwrap();
        assert!(OutputLocks::acquire(&[dir.join("sub/../new.csv")]).is_err());
        drop(lock);
        drop(OutputLocks::acquire(&[output]).unwrap());
        assert_eq!(fs::read(input).unwrap(), b"keep");
        fs::remove_dir_all(dir).unwrap();
    }
}

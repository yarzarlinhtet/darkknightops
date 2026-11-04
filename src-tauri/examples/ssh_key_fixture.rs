//! Disposable loopback fixture credentials only; never reads production SSH keys.
use remotedock::infrastructure::ssh_keys::generate;
use std::{fs::OpenOptions, io::Write, path::Path};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.ssh-test");
    for (name, rsa, secret) in [
        ("generated-ed25519", false, None),
        ("generated-rsa", true, Some("fixture-passphrase")),
    ] {
        let path = directory.join(name);
        if path.exists() {
            continue;
        }
        let key = generate(rsa, secret).map_err(|_| "Fixture generation failed")?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path)?.write_all(key.private.as_bytes())?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.with_extension("pub"))?
            .write_all(key.public.as_bytes())?;
    }
    Ok(())
}

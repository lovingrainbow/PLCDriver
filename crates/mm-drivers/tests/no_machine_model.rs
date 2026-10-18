use std::process::Command;

#[test]
fn drivers_do_not_depend_on_machine_model() {
    let out = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--offline",
            "-p",
            "mm-drivers",
            "--edges",
            "normal,build",
            "--prefix",
            "none",
            "--color",
            "never",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("無法執行 cargo tree");
    assert!(
        out.status.success(),
        "cargo tree 失敗: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    let names: Vec<&str> = text
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert!(
        !names.contains(&"machine-model"),
        "mm-drivers 不應依賴 machine-model"
    );
    assert!(
        names.contains(&"plc-common"),
        "mm-drivers 應依賴 plc-common"
    );
}

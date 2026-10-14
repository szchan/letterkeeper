// SPDX-License-Identifier: GPL-3.0-only
use std::process::Command;

#[test]
fn account_add_interactive_save_is_configuration_only() {
    let script = r#"
import fcntl,json,os,select,struct,subprocess,sys,tempfile,termios,time
from pathlib import Path
with tempfile.TemporaryDirectory() as tmp:
    config=Path(tmp)/'config'; data=Path(tmp)/'data'
    master,slave=os.openpty()
    fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',12,24,0,0))
    child='import fcntl,os,sys,termios; fcntl.ioctl(0,termios.TIOCSCTTY,0); os.execv(sys.argv[1],sys.argv[1:])'
    p=subprocess.Popen([sys.executable,'-c',child,sys.argv[1],'--config-dir',str(config),'--data-dir',str(data),'account','add'],stdin=slave,stdout=slave,stderr=slave,start_new_session=True)
    os.write(master,b'demo\nsender@example.test\npassword\n\nimap.example.test\n\nsmtp.example.test\n\nINBOX\n\n\n\n\nyes\n')
    output=bytearray(); deadline=time.monotonic()+10
    while time.monotonic()<deadline:
        if select.select([master],[],[],.1)[0]:
            try: output.extend(os.read(master,65536))
            except OSError: break
        if p.poll() is not None: break
    if p.poll() is None: p.kill()
    p.wait(); os.close(master);os.close(slave)
    assert (config/'demo.json').exists(), 'confirmed interactive wizard did not save configuration'
    cfg=json.loads((config/'demo.json').read_text())
    assert cfg['permissions']=={'read':True,'draft':False,'send':False}
    assert b'configuration_saved' in output and b'unverified' in output
    assert not any(data.rglob('*.sqlite*')), 'wizard unexpectedly initialized runtime database'
"#;
    let output = Command::new("python3")
        .args(["-c", script, env!("CARGO_BIN_EXE_letterkeeper")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn account_add_rejects_pipes_before_storage_creation() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config");
    let data = temp.path().join("data");
    let result = Command::new(env!("CARGO_BIN_EXE_letterkeeper"))
        .arg("--config-dir")
        .arg(&config)
        .arg("--data-dir")
        .arg(&data)
        .args(["--account", "missing", "account", "add"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("interactive_terminal_required"));
    assert!(!config.exists() && !data.exists());
}

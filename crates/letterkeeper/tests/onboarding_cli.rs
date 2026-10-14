// SPDX-License-Identifier: GPL-3.0-only
use std::process::Command;

#[test]
fn account_add_interactive_save_is_configuration_only() {
    let script = r#"
import fcntl,json,os,select,struct,subprocess,sys,tempfile,termios,time
from pathlib import Path
binary=sys.argv[1]
password=['demo','sender@example.test','password','','imap.example.test','','smtp.example.test','','','INBOX,宽字符文件夹','','','','yes']
gmail=['gmail-demo','sender@example.test','gmail','','synthetic.apps.googleusercontent.com','yes','INBOX','','','send','yes']

def launch(config,data,answers,width=24,eof=False):
    master,slave=os.openpty()
    fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',12,width,0,0))
    before=termios.tcgetattr(slave)
    child='import fcntl,os,sys,termios; fcntl.ioctl(0,termios.TIOCSCTTY,0); os.execv(sys.argv[1],sys.argv[1:])'
    p=subprocess.Popen([sys.executable,'-c',child,binary,'--config-dir',str(config),'--data-dir',str(data),'--account','unused','account','add'],stdin=slave,stdout=slave,stderr=slave,start_new_session=True)
    payload=('\n'.join(answers)+'\n').encode() if answers else b''
    os.write(master,payload)
    output=bytearray();deadline=time.monotonic()+10; sent_eof=False
    while time.monotonic()<deadline:
        if select.select([master],[],[],.05)[0]:
            try: output.extend(os.read(master,65536))
            except OSError: break
        # Keep stdin open during ordinary interaction. For EOF tests send the
        # terminal EOF only once all prior complete lines have been consumed.
        if eof and not sent_eof and output.count(b': ')>=len(answers)+1:
            os.write(master,b'\x04');sent_eof=True
        if p.poll() is not None: break
    if p.poll() is None: p.kill();raise AssertionError('wizard did not finish')
    code=p.wait()
    while select.select([master],[],[],0)[0]:
        try: output.extend(os.read(master,65536))
        except OSError: break
    assert termios.tcgetattr(slave)==before
    os.close(master);os.close(slave)
    return code,bytes(output)

with tempfile.TemporaryDirectory() as tmp:
    root=Path(tmp)
    for width in [24,80]:
        config=root/str(width)/'config';data=root/str(width)/'data'
        code,output=launch(config,data,password,width)
        assert code==0,'wizard save failed'
        assert (config/'demo.json').exists(), 'confirmed interactive wizard did not save configuration'
        cfg=json.loads((config/'demo.json').read_text())
        assert cfg['permissions']=={'read':True,'draft':False,'send':False}
        assert cfg['allowed_folders']==['INBOX','宽字符文件夹']
        assert '宽字符文件夹'.encode() in output
        assert b'configuration_saved' in output and b'unverified' in output
        assert b'account password demo' in output and b'--account demo tui --read-only' in output
        assert not any(data.rglob('*.sqlite*')), 'wizard unexpectedly initialized runtime database'
        original=(config/'demo.json').read_bytes()
        changed=password.copy();changed[1]='different@example.test'
        code,output=launch(config,data,changed,width)
        assert code!=0 and b'account already exists; existing configuration was not replaced' in output
        assert (config/'demo.json').read_bytes()==original,'duplicate wizard overwrote account'
    for provider_answers in [password,gmail]:
        for i in range(len(provider_answers)):
            for mode in ['eof','cancel']:
                config=root/(provider_answers[0]+mode+str(i))/'config'; data=config.parent/'data'
                answers=provider_answers[:i]+(['/cancel'] if mode=='cancel' else [])
                code,output=launch(config,data,answers,eof=mode=='eof')
                assert code==0 and b'cancelled' in output
                assert not config.exists() and not data.exists(), 'cancellation created storage'
    for confirm in ['', 'y', 'YES', 'no', ' yes', 'yes ']:
        config=root/('confirm'+confirm)/'config';data=root/('confirm'+confirm)/'data'
        answers=password[:-1]+[confirm]
        code,output=launch(config,data,answers)
        assert code==0 and not config.exists() and not data.exists()
    config=root/'gmail'/'config';data=root/'gmail'/'data'
    code,output=launch(config,data,gmail,80)
    assert code==0
    cfg=json.loads((config/'gmail-demo.json').read_text())
    assert cfg['imap']=={'host':'imap.gmail.com','port':993}
    assert cfg['smtp']=={'host':'smtp.gmail.com','port':465}
    assert cfg['authentication']['client_secret_ref'] is True
    assert cfg['permissions']=={'read':True,'draft':True,'send':True}
    assert b'account client-secret gmail-demo' in output and b'account login gmail-demo' in output
    assert not any(data.rglob('*.sqlite*'))
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

#[test]
fn doctor_is_safe_json_and_does_not_initialize_selected_account() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("sensitive-config-canary");
    let data = temp.path().join("sensitive-data-canary");
    let result = Command::new(env!("CARGO_BIN_EXE_letterkeeper"))
        .arg("--config-dir")
        .arg(&config)
        .arg("--data-dir")
        .arg(&data)
        .args(["--account", "missing-sensitive-account", "doctor"])
        .env_remove("HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME")
        .env("XDG_RUNTIME_DIR", temp.path().join("missing-runtime"))
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/nonexistent-synthetic-dbus",
        )
        .output()
        .unwrap();
    assert!(!result.status.success());
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["authentication"], "not_checked");
    assert_eq!(
        report["sandbox"], "ready",
        "doctor must execute the real isolated worker"
    );
    assert_eq!(report["environment_ready"], false);
    assert_eq!(report["config_storage"], "not_initialized");
    assert_eq!(report["data_storage"], "not_initialized");
    assert!(!config.exists() && !data.exists());
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    for sensitive in [
        "sensitive-config-canary",
        "sensitive-data-canary",
        "missing-sensitive-account",
    ] {
        assert!(!all.contains(sensitive));
    }
}

#[test]
fn account_add_requires_both_terminal_streams() {
    let script = r#"
import os,subprocess,sys,tempfile
from pathlib import Path
with tempfile.TemporaryDirectory() as tmp:
    for pipe_stdin in [True,False]:
        config=Path(tmp)/str(pipe_stdin)/'config';data=Path(tmp)/str(pipe_stdin)/'data'
        master,slave=os.openpty()
        p=subprocess.Popen([sys.argv[1],'--config-dir',str(config),'--data-dir',str(data),'account','add'],
                           stdin=subprocess.PIPE if pipe_stdin else slave,
                           stdout=slave if pipe_stdin else subprocess.PIPE,stderr=subprocess.PIPE)
        _,error=p.communicate(timeout=5)
        assert p.returncode!=0 and b'interactive_terminal_required' in error
        assert not config.exists() and not data.exists()
        os.close(master);os.close(slave)
"#;
    let result = Command::new("python3")
        .args(["-c", script, env!("CARGO_BIN_EXE_letterkeeper")])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Real DNS/HTTP observers around MCP, editor and full TUI review.

A disposable user/network namespace contains the client and reachable observers.
Only its resolver files are replaced. Production client network policy and the
parser's separate no-network worker are unchanged. No host config is modified.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path
import queue
import re
import select
import socket
import struct
import subprocess
import tempfile
import termios
import threading
import time


def normalized(data):
    return re.sub(rb'\s+', b'', re.sub(rb'\x1b\[[0-?]*[ -/]*[@-~]', b'', data))


def receive(proc, fd, marker):
    data = bytearray()
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if select.select([fd], [], [], .05)[0]:
            data += os.read(fd, 65536)
        if normalized(marker.encode()) in normalized(data):
            return
        assert proc.poll() is None, 'synthetic TUI exited early: ' + repr(bytes(data[-200:]))
        assert len(data) <= 2 * 1024 * 1024, 'terminal output limit'
    raise AssertionError('synthetic TUI marker not displayed')


def inner(binary):
    queries = []
    dns = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    dns.bind(('127.0.0.1', 53))
    dns.settimeout(.1)
    stop = threading.Event()
    def serve_dns():
        while not stop.is_set():
            try:
                packet, address = dns.recvfrom(4096)
            except socket.timeout:
                continue
            except OSError:
                return
            if len(packet) < 17:
                continue
            pos = 12
            while pos < len(packet) and packet[pos]:
                pos += 1 + packet[pos]
            end = pos + 5
            if end > len(packet):
                continue
            questions = packet[12:end]
            queries.append(questions)
            kind = packet[end - 4:end - 2]
            answer = b''
            if kind == b'\x00\x01':
                answer = b'\xc0\x0c\x00\x01\x00\x01\x00\x00\x00\x00\x00\x04\x7f\x00\x00\x01'
            response = packet[:2] + b'\x81\x80\x00\x01' + struct.pack('!H', bool(answer)) + b'\x00\x00\x00\x00' + questions + answer
            dns.sendto(response, address)
    thread = threading.Thread(target=serve_dns, daemon=True)
    thread.start()
    http = socket.socket()
    http.bind(('127.0.0.1', 0))
    http.listen()
    http.settimeout(2)
    # Actual resolver positive control, not a datagram sent to an unused port.
    assert socket.gethostbyname('positive.letterkeeper.test') == '127.0.0.1'
    assert queries, 'DNS resolver observer was not reached'
    connection = socket.create_connection(('positive.letterkeeper.test', http.getsockname()[1]))
    accepted, _ = http.accept()
    connection.close()
    accepted.close()
    queries.clear()
    http.setblocking(False)
    with tempfile.TemporaryDirectory(prefix='letterkeeper-privacy-') as tmp:
        root = Path(tmp)
        state = root / 'private'
        target = f'http://resource.letterkeeper.test:{http.getsockname()[1]}/pixel'
        dangerous = f'<img src="{target}"><link rel="dns-prefetch" href="//resource.letterkeeper.test"><script>fetch("{target}")</script>'
        mcp = subprocess.Popen([str(binary), '--state-dir', str(state), 'mcp'],
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=open(root / "mcp-error.log", "w"), text=True)
        incoming = queue.Queue()
        def read_frames():
            for line in mcp.stdout:
                incoming.put(json.loads(line))
            incoming.put(None)
        threading.Thread(target=read_frames, daemon=True).start()
        serial = 0
        def rpc(method, params):
            nonlocal serial
            serial += 1
            mcp.stdin.write(json.dumps({'jsonrpc':'2.0','id':serial,'method':method,'params':params})+'\n')
            mcp.stdin.flush()
            while True:
                row = incoming.get(timeout=10)
                assert row is not None, 'MCP stream closed: ' + (root / 'mcp-error.log').read_text()
                if row.get('id') == serial:
                    assert 'error' not in row, 'MCP request rejected'
                    return row['result']
        def tool(name, arguments):
            result = rpc('tools/call', {'name':name,'arguments':arguments})
            assert not result.get('isError'), 'synthetic tool failed'
            return json.loads(result['content'][0]['text'])
        try:
            rpc('initialize', {'protocolVersion':'2025-06-18','capabilities':{},'clientInfo':{'name':'privacy-verifier','version':'0.1'}})
            mcp.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
            mcp.stdin.flush()
            body = dangerous + '\n' + ('中文长正文\n' * 1600) + 'TAIL_PRIVACY'
            assert 16384 < len(body.encode()) <= 32768
            draft = tool('create_draft', {'to':['recipient@example.test'],'subject':'Privacy draft','body':body})
            assert draft['untrusted']
            document = tool('get_draft', {'id':str(draft['id'])})
            assert document['untrusted'] and target not in json.dumps(document)
        finally:
            mcp.stdin.close()
            mcp.wait(timeout=5)
            mcp.stdout.close()
        master, slave = os.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH',40,120,0,0))
        before = termios.tcgetattr(slave)
        proc = subprocess.Popen(['/usr/bin/python3','/verify.py',str(binary),'--tui-child',str(state)],
                                stdin=slave,stdout=slave,stderr=slave,start_new_session=True,
                                env={**os.environ,'TERM':'xterm-256color'})
        try:
            receive(proc,master,'LetterKeeper')
            os.write(master,b'd')
            receive(proc,master,'Privacy draft')
            os.write(master,b'\r')
            receive(proc,master,'Ctrl+S')
            # Editor is already showing the malicious-looking string as plain data.
            os.write(master,b'\x1b[17~')
            receive(proc,master,'完整审阅')
            os.write(master,b'\x1b[F')
            receive(proc,master,'TAIL_PRIVACY')
            os.write(master,b'q')
            assert proc.wait(timeout=5) == 0
            assert termios.tcgetattr(slave) == before
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait()
            os.close(master)
            os.close(slave)
        time.sleep(.15)
        try:
            connection, _ = http.accept()
        except BlockingIOError:
            pass
        else:
            connection.close()
            raise AssertionError('body resource HTTP request observed')
        assert not queries, 'body-stage DNS request observed'
    stop.set()
    thread.join(timeout=1)
    dns.close()
    http.close()
    print(json.dumps({'result':'passed','phases':['MCP stdio','TUI editor','full TUI review'],
                      'positive_controls':['real DNS resolution','HTTP connection'],
                      'body_stage_dns_queries':0,'body_resource_http_connections':0,
                      'host_config_modified':False,'data':'synthetic only'}))


def outer(binary):
    with tempfile.TemporaryDirectory(prefix='letterkeeper-privacy-resolver-') as tmp:
        root = Path(tmp)
        (root/'resolv.conf').write_text('nameserver 127.0.0.1\noptions timeout:1 attempts:1\n')
        (root/'nsswitch.conf').write_text('hosts: files dns\n')
        (root/'hosts').write_text('127.0.0.1 localhost\n')
        script = Path(__file__).resolve()
        command = ['/usr/bin/bwrap','--die-with-parent','--new-session','--unshare-user','--uid','0','--gid','0','--unshare-net','--cap-add','CAP_NET_BIND_SERVICE',
                   '--ro-bind','/usr','/usr','--symlink','usr/lib','/lib','--symlink','usr/lib','/lib64',
                   '--proc','/proc','--dev','/dev','--tmpfs','/tmp','--ro-bind',str(binary.parent),str(binary.parent),
                   '--ro-bind',str(script),'/verify.py','--ro-bind',str(root/'resolv.conf'),'/etc/resolv.conf',
                   '--ro-bind',str(root/'nsswitch.conf'),'/etc/nsswitch.conf','--ro-bind',str(root/'hosts'),'/etc/hosts',
                   '/usr/bin/python3','/verify.py',str(binary),'--inside']
        subprocess.run(command,check=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary',type=Path)
    parser.add_argument('--inside',action='store_true',help=argparse.SUPPRESS)
    parser.add_argument('--tui-child',type=Path,help=argparse.SUPPRESS)
    args=parser.parse_args()
    if args.tui_child:
        fcntl.ioctl(0,termios.TIOCSCTTY,0)
        os.execv(str(args.binary),[str(args.binary),'--state-dir',str(args.tui_child),'tui'])
    (inner if args.inside else outer)(args.binary.resolve())

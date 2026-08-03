## NeuroSkill + WSL Findings Summary

I was trying to use the NeuroSkill CLI from WSL to connect to the NeuroSkill desktop app/daemon running on Windows.

### Environment

* Windows host running NeuroSkill desktop app
* WSL distro: `RelectiveChicken`
* WSL initially defaulted to `root`
* Later created a normal WSL user: `jelani`
* `.wslconfig` includes:

```ini
[wsl2]
networkingMode=mirrored

[experimental]
hostAddressLoopback=true
```

### Initial WSL startup warning

After adding `.wslconfig` and restarting WSL, I saw:

```text
wsl: Failed to start the systemd user session for 'root'. See journalctl for more details.
```

However, this did not recur on the next WSL start, so it appears to have been incidental or startup-timing-related rather than the main issue.

### Port discovery

I initially tested the documented/default port:

```text
127.0.0.1:8375
```

PowerShell showed that nothing was listening there:

```text
TcpTestSucceeded : False
```

Running the NeuroSkill CLI from Windows revealed that the actual daemon was running on a different port:

```text
127.0.0.1:18444
ws://127.0.0.1:18444
```

Windows CLI successfully connected and reported the Muse device as connected, including EEG/PPG/IMU samples and scores.

Conclusion: NeuroSkill was running, but not on port `8375`; it had selected/discovered port `18444`.

### Windows behavior

From PowerShell:

```powershell
npx neuroskill status
```

Result:

```text
probing daemon at 127.0.0.1:18444…
daemon reachable at port 18444
auto-transport: probing WebSocket…
transport: WebSocket ws://127.0.0.1:18444
```

The command succeeded and returned device/session/status data.

### WSL behavior as root

From WSL as root:

```bash
npx neuroskill status
```

Result:

```text
probing daemon at 127.0.0.1:18444…
daemon reachable at port 18444
auto-transport: probing WebSocket…
WebSocket unavailable — transport: HTTP http://127.0.0.1:18444
⚡ status
error: server returned ok=false: undefined
```

This proved that WSL could reach the Windows daemon at `127.0.0.1:18444`, so mirrored networking and `hostAddressLoopback` were working. The issue was not basic network reachability.

### Raw WebSocket test

From WSL:

```bash
npx wscat -c ws://127.0.0.1:18444
```

Result:

```text
error: Unexpected server response: 401
```

The same raw `wscat` test from Windows PowerShell also returned:

```text
error: Unexpected server response: 401
```

Conclusion: `wscat` is not a valid test client for NeuroSkill’s daemon. The daemon likely expects some CLI-specific handshake metadata, token, origin, subprotocol, or session detail. A raw WebSocket connection being rejected with `401` is expected and does not by itself indicate a WSL networking problem.

### WSL user test

Because WSL defaulted to root, I created a normal Linux user:

```bash
adduser jelani
usermod -aG sudo jelani
```

The first attempt to run NeuroSkill as `jelani` failed because that user had Node v12:

```text
required: { node: '>=18' }
current: { node: 'v12.22.9' }
SyntaxError: Unexpected token '.'
```

After updating Node/npm for the `jelani` user, the CLI ran successfully, but the connection behavior was the same as root:

```text
probing daemon at 127.0.0.1:18444…
daemon reachable at port 18444
auto-transport: probing WebSocket…
WebSocket unavailable — transport: HTTP http://127.0.0.1:18444
⚡ status
error: server returned ok=false: undefined
```

Conclusion: the issue is not simply caused by running WSL as root.

### Final diagnosis

The current findings are:

```text
Windows NeuroSkill CLI → daemon reachable, WebSocket works, status succeeds
WSL NeuroSkill CLI as root → daemon reachable, WebSocket unavailable, HTTP fallback fails
WSL NeuroSkill CLI as jelani → daemon reachable, WebSocket unavailable, HTTP fallback fails
Raw wscat from Windows → 401
Raw wscat from WSL → 401
```

So the likely issue is not `.wslconfig`, localhost routing, WSL mirrored networking, or root vs non-root.

The most likely explanation is that the NeuroSkill CLI behaves differently under WSL/Linux when connecting to the Windows desktop daemon. It can discover and reach the daemon port, but it cannot complete the same WebSocket transport/session flow that works from the Windows CLI. The HTTP fallback reaches the daemon but returns `ok=false`.

### Practical workaround

From inside WSL, use the Windows-side NeuroSkill CLI via PowerShell:

```bash
powershell.exe -NoProfile -Command "npx neuroskill status"
```

This works around the WSL/Linux WebSocket transport issue by running the CLI in the Windows user context, where NeuroSkill already works.

### Suggested issue framing

This appears to be a NeuroSkill CLI/daemon compatibility issue when calling the Windows desktop daemon from WSL. WSL can reach the daemon over localhost, but the Linux/WSL CLI cannot complete WebSocket transport and falls back to HTTP, which returns `ok=false`.


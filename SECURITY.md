# Security Policy

## Supported versions

Open-Lid is currently pre-1.0. Only the latest released version is supported
with security fixes. Once a 1.x line ships, this policy will be updated to
specify a longer support window.

| Version    | Supported          |
|------------|--------------------|
| Latest 0.x | :white_check_mark: |
| Earlier    | :x:                |

## Threat model

Open-Lid runs a privileged helper daemon as `root` (via launchd) so it can
toggle the system `pmset disablesleep` setting. This means a vulnerability
in Open-Lid that allows arbitrary code execution would run as root on the
user's machine. Take security reports seriously.

Specifically in scope:

- **Privilege escalation** — a non-root local user causing the helper to
  perform actions on their behalf without proper authorization
- **Helper impersonation** — a malicious local process pretending to be the
  Open-Lid app and instructing the helper to manipulate sleep state
- **Memory corruption / unsoundness** in unsafe FFI code (IOKit, NSXPC,
  SMAppService bindings)
- **Supply chain** — dependency vulnerabilities surfaced via `cargo audit`
- **Data exfiltration** — Open-Lid does not collect, transmit, or store
  any user data; reports of any data leaving the user's machine are
  high-priority

Specifically out of scope:

- **User installs malicious app pretending to be Open-Lid** — covered by
  macOS Gatekeeper / notarization, not our application
- **User runs Open-Lid as root** (`sudo open-lid`) — Open-Lid is not
  designed to be run as root; the helper is the only root component
- **System sleep policy after legitimate `pmset disablesleep` call** — that's
  the documented behavior of Open-Lid

## Reporting a vulnerability

**Do not file public GitHub issues for security vulnerabilities.**

Email security reports to **diyan.bogdanov@gmail.com** with subject line
prefixed `[Open-Lid Security]`. Include:

- A description of the vulnerability
- Steps to reproduce, or a proof-of-concept
- Open-Lid version (`open-lid --version`)
- macOS version (`sw_vers -productVersion`)
- Your assessment of impact

If you prefer encrypted email, ask in the initial message and I'll provide
a PGP key.

### Expected response timeline

- **Acknowledgement:** within 7 calendar days
- **Initial assessment:** within 14 calendar days
- **Fix or status update:** within 30 calendar days for confirmed
  vulnerabilities

I aim to be faster than these limits, but as a solo maintainer in spare
time, they're the limits I can commit to.

### Disclosure policy

- I follow a **coordinated disclosure** model.
- Once a fix is ready, I'll publish a release and a security advisory on the
  GitHub Security Advisories page.
- I'll credit you in the advisory unless you ask to remain anonymous.
- If a vulnerability is being actively exploited, I'll prioritize release
  speed over coordination — the public deserves a fix.

### Bug bounty

Open-Lid is a free, solo-maintained project. I don't have a bounty program.
Reports are appreciated regardless.

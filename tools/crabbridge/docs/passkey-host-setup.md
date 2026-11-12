# Optional passkey host setup

The default binary cannot enable passkeys and rejects a configuration containing passkey settings. Build explicitly with `--features experimental-device-auth`. This is an opt-in, reviewed-library path, not permission to deploy the frontend or enroll anyone's real credential.

For an approved trusted HTTPS origin, the host administrator can run:

```
codex-harbor device-init CONFIG https://bridge.example.test bridge.example.test /absolute/new/private/ledger
codex-harbor device-list CONFIG USER
codex-harbor device-remove CONFIG USER HEX_CREDENTIAL_ID
```

The example hostname is reserved for local browser test interception; it is not a live deployment. Initialization requires an existing user configuration, refuses an existing ledger directory or existing passkey settings, and creates a durable authority and distinct user IDs. Restart gateways after configuration changes. Never put the ledger inside a web-served directory. Exact canonical HTTPS origin and RP are host-selected, not supplied by an invitation or browser.

Registration requires an authenticated idle connection, explicit action, and a freshly entered current password. A registered passkey can then replace the password step on a **new** authenticated Noise connection. A fresh private invitation is still required; it is not persisted. The browser uses its standard passkey API and user verification, which may display a biometric/security-key prompt. Synced passkeys are not necessarily tied to one physical computer.

Host and browser listings use hexadecimal IDs accepted by removal. Browser inventory is scoped to the current user, active/unexpired credentials, four entries per page. Host inventory includes revoked tombstones. Removal denies new authentication and is rechecked by existing sessions on commands, delayed tool installation and idle cleanup. It cannot undo work already sent to a tool.

Changing/replacing a user's password through the host CLI or removing the user revokes the old UUID before writing new configuration. A subsequent configuration save failure leaves prior passkeys revoked; it does not roll back the revocation. A manual password edit cannot silently inherit a mismatched ledger's authentication authority. Recovery requires an explicit host-admin decision, not automatic ledger recreation.

SQLite lock waits are bounded and failure denies authority. Local filesystem I/O is not guaranteed realtime; use a private local host-owned filesystem. There is no promise of atomic configuration changes across all host processes or rollback protection against restoring an old private database backup.

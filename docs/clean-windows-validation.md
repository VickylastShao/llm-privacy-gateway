# Clean Windows validation

Use a disposable Windows VM or a clean local Windows account. The account must not already contain
an LLM Privacy Gateway installation, uninstall record, or application-data directory.

## Automated install smoke test

Run PowerShell without elevation:

```powershell
.\scripts\windows-clean-install-smoke.ps1 `
  -InstallerPath ".\LLM Privacy Gateway_0.1.0_x64-setup.exe" `
  -ExpectedSha256 "<SHA256SUMS value>" `
  -AppDataDirectoryName "com.vickylast.llmprivacygateway" `
  -RequireSigned `
  -ConfirmCleanMachine
```

The script verifies the checksum and signatures, refuses a non-clean target, performs a silent
current-user install, checks the Start Menu shortcut, launches the GUI, verifies normal stopped-state
window exit, uninstalls, and confirms that uninstall preserved the local SQLite database. It leaves
the test application-data directory in the disposable account as evidence.

## Manual tray and upgrade checks

The following interactions require direct use of the Windows shell:

1. Install and launch the signed package from the Start Menu. Confirm that no console window opens.
2. Add a disposable loopback connection with an arbitrary non-production key and start protection.
3. Close the main window. Confirm the process remains running and the tray icon remains visible.
4. Use **Show** from the tray and confirm the same window reopens.
5. Close it again, use **Exit**, and confirm the gateway and desktop process stop.
6. Reinstall the same signed version, then install the next signed version. Confirm the disposable
   connection and its operating-system credentials still load.
7. Delete the disposable connection in the application, uninstall, and reset or discard the test
   account/VM.

Record the Windows version, architecture, display scaling, installer SHA-256, signature subject,
and pass/fail result for each step in the release evidence.

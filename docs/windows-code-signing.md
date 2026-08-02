# Windows code-signing release setup

The public release workflow requires an Authenticode certificate with a private key and the Code
Signing enhanced key usage. Do not commit the PFX, its Base64 representation, or its password.

## GitHub release environment

1. Create a protected GitHub environment named `release` and require manual approval.
2. Add environment secret `WINDOWS_CERTIFICATE` containing the Base64 encoding of the complete PFX
   byte sequence without surrounding quotes.
3. Add environment secret `WINDOWS_CERTIFICATE_PASSWORD` containing the PFX password.
4. Keep Actions access restricted to the release workflow and trusted maintainers.

The workflow writes the PFX only to the ephemeral runner temporary directory, imports it into the
current-user certificate store, immediately removes the PFX file, builds the application, and runs
`signtool verify /pa /all` on both the application executable and generated installer.

## Local certificate validation

Before configuring GitHub, inspect the certificate on a trusted Windows machine:

```powershell
$password = Read-Host "PFX password" -AsSecureString
$certificate = Get-PfxCertificate -FilePath .\codesigning.pfx -Password $password
$certificate | Format-List Subject, Thumbprint, NotBefore, NotAfter, EnhancedKeyUsageList
```

Confirm that the subject matches the permanent publisher identity, the certificate is within its
validity period, the private key is present, and Code Signing appears in `EnhancedKeyUsageList`.

## Release verification

After the draft release is generated:

```powershell
signtool verify /pa /all .\lpg-desktop.exe
signtool verify /pa /all ".\LLM Privacy Gateway_0.1.0_x64-setup.exe"
Get-FileHash ".\LLM Privacy Gateway_0.1.0_x64-setup.exe" -Algorithm SHA256
```

The checksum must match `SHA256SUMS.txt`. Do not publish the draft if either signature, timestamp,
checksum, or SPDX SBOM verification fails.

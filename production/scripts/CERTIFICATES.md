# Certificate Management Scripts - Quick Start Guide

This directory contains scripts for managing TLS certificates for the MPC Wallet production environment.

## Quick Start

### 1. Generate Certificates (First Time)

**Linux/macOS:**
```bash
cd production/scripts
./generate-certs.sh 5
```

**Windows PowerShell:**
```powershell
cd production\scripts
.\generate-certs.ps1 -NumNodes 5
```

This creates:
- Root CA certificate and key (`ca.crt`, `ca.key`)
- 5 node certificates and keys (`node1.crt`, `node1.key`, etc.)

### 2. Verify Certificates

```bash
./verify-certs.sh
```

Check certificate validity, signatures, and expiry dates.

### 3. Renew Certificates

Renew all node certificates:
```bash
./renew-certs.sh all
```

Renew specific node:
```bash
./renew-certs.sh 3  # Renew node-3
```

## Available Scripts

| Script | Purpose | Platform |
|--------|---------|----------|
| `generate-certs.sh` | Generate all certificates | Linux/macOS/Git Bash |
| `generate-certs.ps1` | Generate all certificates | Windows PowerShell |
| `renew-certs.sh` | Renew node certificates | Linux/macOS/Git Bash |
| `verify-certs.sh` | Verify certificate validity | Linux/macOS/Git Bash |
| `certs-common.sh` | Shared functions (sourced by other scripts) | Linux/macOS/Git Bash |

## Common Tasks

### Generate Certificates for 10 Nodes

```bash
./generate-certs.sh 10
```

### Generate with Encrypted CA Key

```bash
./generate-certs.sh 5 --encrypt-ca
```

You'll be prompted for a passphrase. This adds extra security to the CA private key.

### Check Certificates Expiring Soon

```bash
./verify-certs.sh --warn-days 60
```

This warns about certificates expiring in the next 60 days.

### Verbose Certificate Information

```bash
./verify-certs.sh --verbose
```

Shows detailed certificate information including key sizes, algorithms, and full validity dates.

### Renew Without Backup

```bash
./renew-certs.sh all --no-backup
```

Renews certificates without creating backups (not recommended for production).

## File Locations

After running `generate-certs.sh`, files are created in:

```
production/certs/
├── .gitignore              # Prevents accidental key commits
├── README.md               # Comprehensive certificate documentation
├── ca.crt                  # Root CA certificate (10-year validity)
├── ca.key                  # Root CA private key (KEEP SECURE!)
├── node1.crt               # Node 1 certificate (1-year validity)
├── node1.key               # Node 1 private key
├── node2.crt               # Node 2 certificate
├── node2.key               # Node 2 private key
├── ...                     # Additional nodes
└── backups/                # Backup directory (created during renewal)
    └── YYYYMMDD_HHMMSS/    # Timestamped backups
```

## Security Notes

**CRITICAL:**
- Never commit `.key` files to git (`.gitignore` is configured to prevent this)
- Keep `ca.key` in a secure location with backups
- Use `--encrypt-ca` flag for production environments
- Set proper file permissions: `chmod 600 certs/*.key`

**Best Practices:**
- Backup `ca.key` to encrypted offline storage
- Rotate node certificates before they expire (yearly)
- Use separate CAs for dev/staging/production
- Monitor expiry with `verify-certs.sh --warn-days 60`

## Deploying to Docker Nodes

After generating certificates, copy them to Docker containers:

```bash
# For node-1
docker cp certs/ca.crt node-1:/certs/
docker cp certs/node1.crt node-1:/certs/
docker cp certs/node1.key node-1:/certs/

# For node-2
docker cp certs/ca.crt node-2:/certs/
docker cp certs/node2.crt node-2:/certs/
docker cp certs/node2.key node-2:/certs/

# Repeat for other nodes...
```

Or mount as volumes in `docker-compose.yml`:

```yaml
services:
  node-1:
    volumes:
      - ./certs/ca.crt:/certs/ca.crt:ro
      - ./certs/node1.crt:/certs/node1.crt:ro
      - ./certs/node1.key:/certs/node1.key:ro
```

## Troubleshooting

### "OpenSSL not found"

**Linux (Ubuntu/Debian):**
```bash
sudo apt-get install openssl
```

**macOS:**
```bash
brew install openssl
```

**Windows:**
- Download from: https://slproweb.com/products/Win32OpenSSL.html
- Or use Chocolatey: `choco install openssl`

### Certificate Verification Failed

Check if certificates are signed by the correct CA:

```bash
openssl verify -CAfile certs/ca.crt certs/node1.crt
```

If this fails, regenerate the node certificate:

```bash
./renew-certs.sh 1
```

### Certificate Expired

Renew the expired certificates:

```bash
./renew-certs.sh all
./verify-certs.sh
```

### Permission Denied Errors

Fix file permissions:

```bash
chmod 600 certs/*.key
chmod 644 certs/*.crt
```

**Note:** On Windows with Git Bash, permission settings may not work as expected. This is a known limitation.

### Git Bash Path Conversion Issues (Windows)

If you encounter path conversion errors on Windows, the scripts use `//C=` prefix in certificate subjects to work around Git Bash path conversion. This may cause harmless warnings like:

```
req warning: Skipping unknown subject name attribute "/C"
```

This is expected and doesn't affect certificate functionality. The CN (Common Name) is still set correctly.

## Requirements

- OpenSSL (version 1.1.1 or higher)
- Bash (Linux/macOS/Git Bash on Windows)
- PowerShell (for Windows .ps1 scripts)

## Script Help

All scripts support `--help` flag:

```bash
./generate-certs.sh --help
./renew-certs.sh --help
./verify-certs.sh --help
```

## Certificate Specifications

### Root CA Certificate
- Algorithm: RSA 4096-bit
- Hash: SHA256
- Validity: 10 years (3650 days)
- Subject: `/C=US/ST=State/L=City/O=MPC-Wallet/CN=MPC-Wallet-Root-CA`

### Node Certificates
- Algorithm: RSA 2048-bit
- Hash: SHA256
- Validity: 1 year (365 days)
- Subject: `/C=US/ST=State/L=City/O=MPC-Wallet/CN=node-{id}`

## Further Documentation

For comprehensive documentation, security best practices, and troubleshooting:

- See `production/certs/README.md` for detailed certificate documentation
- Check individual script comments for advanced options
- Review OpenSSL documentation: https://www.openssl.org/docs/

## Support

For issues or questions:
1. Check `production/certs/README.md`
2. Run `./verify-certs.sh --verbose` for diagnostics
3. Review script source code comments
4. Contact the infrastructure team

---

**Created:** 2026-01-20
**Version:** 1.0.0

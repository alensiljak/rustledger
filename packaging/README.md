# Linux Package Distribution

This directory contains packaging files for Linux distributions.

## Fedora/RHEL (COPR)

**Install:**

```bash
sudo dnf copr enable rustledger/rustledger
sudo dnf install rustledger
```

**Package location:** `rpm/rustledger.spec`

**COPR project:** https://copr.fedorainfracloud.org/coprs/rustledger/rustledger/

### Setup (maintainer)

1. Create Fedora account at https://accounts.fedoraproject.org/
1. Go to https://copr.fedorainfracloud.org/
1. Create new project "rustledger" with settings:
   - Chroots: `fedora-rawhide-x86_64`, `fedora-rawhide-aarch64`, `fedora-41-*`, `fedora-40-*`
   - Build options: Enable "Internet access during builds" for cargo
1. Add a package using SCM integration:
   - Package name: `rustledger`
   - SCM type: `git`
   - Clone URL: `https://github.com/rustledger/rustledger.git`
   - Committish: Leave empty (uses latest tag)
   - Spec file: `packaging/rpm/rustledger.spec`
   - Source type: `SCM`
1. Get API token from https://copr.fedorainfracloud.org/api/
1. Add `COPR_API_TOKEN` secret to GitHub repository

## Ubuntu/Debian (PPA)

**Install:**

```bash
sudo add-apt-repository ppa:robcohen/rustledger
sudo apt update
sudo apt install rustledger
```

**Package location:** `debian/` (TODO: not yet implemented)

**PPA:** https://launchpad.net/~robcohen/+archive/ubuntu/rustledger

### Setup (maintainer)

1. Create Launchpad account at https://launchpad.net/
1. Create PPA at https://launchpad.net/~/+activate-ppa named "rustledger"
1. Generate GPG key for signing:
   ```bash
   gpg --full-generate-key  # RSA 4096, no expiry
   gpg --armor --export-secret-keys KEY_ID > launchpad.gpg
   ```
1. Upload GPG public key to keyservers and Launchpad
1. Generate SSH key for uploads:
   ```bash
   ssh-keygen -t ed25519 -C "rustledger-ci@launchpad" -f launchpad_ssh
   ```
1. Add SSH public key to Launchpad profile
1. Add GitHub secrets:
   - `LAUNCHPAD_GPG_PRIVATE_KEY`: Content of launchpad.gpg
   - `LAUNCHPAD_SSH_PRIVATE_KEY`: Content of launchpad_ssh

## Version Format

- RPM: `1.0.0~rc.18` (tilde for prereleases, sorts before 1.0.0)
- DEB: `1.0.0~rc.18-1` (tilde + debian revision)

## Migration to Official Repos

### Fedora Official

1. File Package Review Request in Bugzilla
1. Find sponsor to review
1. Package spec follows Fedora guidelines (already compatible)

### Debian Official

1. File ITP (Intent To Package) bug
1. Find Debian Developer sponsor
1. Package follows Debian Policy (already compatible)
1. Once in Debian → auto-syncs to Ubuntu

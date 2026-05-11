# ScroogeDLP — Dev Environment (Mac + Ubuntu UTM VM)

Workflow do rozwoju ScroogeDLP cross-platform. **Mac jest stacją developerską**
(kod + manager), **Ubuntu w UTM VM jest testbed'em agenta**.

## Po co VM?

Manager jest "po prostu serwerem" — działa wszędzie gdzie jest Linux+Docker.
Agent natomiast musi działać natywnie na docelowym systemie operacyjnym
(Linux/macOS/Windows), bo zbiera dane systemowe (file system events,
USB hotplug, clipboard…). UTM VM pozwala testować Linux agenta z Maca
bez dual-bootowania ani osobnej maszyny.

---

## 1. Instalacja UTM (jednorazowo)

1. Pobierz **UTM** z <https://mac.getutm.app> (darmowy, otwarty;
   wersja z App Store kosztuje, dofinansowuje projekt — to dokładnie ta sama
   aplikacja).
2. `mv UTM.app /Applications/`
3. Pobierz **Ubuntu 22.04 Server LTS** ISO:
   - ARM64 (Apple Silicon): <https://ubuntu.com/download/server/arm>
   - AMD64 (Intel Mac): <https://ubuntu.com/download/server>

## 2. Tworzenie VM

W UTM:

1. **Create New VM** → **Virtualize** → **Linux**
2. Skip "Boot ISO Image" pierwszy raz; doisztie po kreatorze.
3. **Hardware**:
   - RAM: **4 GB** (minimum 2 GB)
   - CPU Cores: **2**
4. **Storage**: **20 GB**
5. **Shared Directory**: pomiń (używamy git'a)
6. **Summary** → nazwa np. `scrooge-ubuntu` → **Save**
7. Edytuj VM (kliknij ołówek) → **Drives** → **New Drive** → **CD/DVD** →
   wybierz pobrany ISO Ubuntu.
8. **Network** → tryb **Shared Network** (to default, ale potwierdź).
   - VM dostanie IP w sieci `192.168.64.x` (typowo).
   - Host (Mac) jest dostępny dla VM jako gateway **`192.168.64.1`**.
9. **Start VM** → przejdź przez instalator Ubuntu (Server, no GUI). Pytania:
   - Język: English
   - Storage: full disk, no LVM (dla prostoty)
   - User: jaki chcesz (np. `sqtx`), zapamiętaj hasło
   - SSH: **Install OpenSSH server** — kluczowe!
   - Featured server snaps: skip wszystko
   - Reboot po instalacji, wyjmij ISO (UTM zwykle sam to robi).

## 3. Pierwsze logowanie + SSH key (z Maca)

W UTM po starcie VM zaloguj się raz w konsoli, sprawdź IP:

```bash
ip -4 addr show enp0s1 | grep inet     # zwykle 192.168.64.X
```

Następnie z **Maca** (Terminal):

```bash
# Wgraj klucz SSH (założenie: masz już ~/.ssh/id_ed25519.pub lub id_rsa.pub):
ssh-copy-id <ubuntu-user>@192.168.64.5    # użyj IP z poprzedniego kroku

# Test:
ssh <ubuntu-user>@192.168.64.5

# Dla wygody — dodaj do ~/.ssh/config:
cat >> ~/.ssh/config <<EOF
Host scrooge-vm
    HostName 192.168.64.5
    User <ubuntu-user>
EOF

# Teraz: ssh scrooge-vm
```

## 4. First-time VM setup (skrypt)

Z VM (przez SSH lub bezpośrednio w konsoli UTM):

```bash
curl -fsSL https://raw.githubusercontent.com/SQTX/Scrooge-DLP/main/deploy/dev/setup-ubuntu-vm.sh -o setup.sh
chmod +x setup.sh
./setup.sh
```

Albo (jeśli wolisz nie wykonywać skryptu z internetu zanim go nie przejrzysz):

```bash
git clone https://github.com/SQTX/Scrooge-DLP.git
cd Scrooge-DLP
less deploy/dev/setup-ubuntu-vm.sh         # przegląd
./deploy/dev/setup-ubuntu-vm.sh
```

Skrypt zrobi:
- `apt install` build deps + protobuf + jq + ufw
- `rustup install` (jako user, nie root) — stable toolchain
- Docker Engine (oficjalne repo, nie apt's outdated)
- `useradd scrooge` (system user, bez shell'a) + katalogi `/etc/scrooge/` i `/var/lib/scrooge/`
- `cp scrooge-agent.service` do `/etc/systemd/system/`
- `cp agent.vm.yaml.example` do `/etc/scrooge/agent.yaml` (template — wymaga edycji)
- `ufw allow ssh`, reszta inbound zablokowana

Po końcu skrypt wypisze **next steps** — postępuj zgodnie.

## 5. Wymiana cert'ów + tokenu (Mac → VM)

### Na Macu:

```bash
cd ~/Projects/App/ScroogeDLP

# (1) Zregeneruj certy z IP Maca w SAN (UTM Shared Network gateway):
rm -f dev-certs/server.pem dev-certs/server.key
make dev-certs        # automat: domyślne SAN obejmuje 192.168.64.1

# (2) Wygeneruj token (lub użyj istniejącego z dev-certs/enrollment-token.txt):
make dev-bootstrap

# (3) Pokaż CA i token:
cat dev-certs/ca.pem
cat dev-certs/enrollment-token.txt
```

### Na VM:

```bash
# Skopiuj CA z Maca:
scp <mac-user>@192.168.64.1:~/Projects/App/ScroogeDLP/dev-certs/ca.pem ~
sudo install -m 0644 -o scrooge -g scrooge ~/ca.pem /etc/scrooge/ca.pem

# Edytuj agent config:
sudo nano /etc/scrooge/agent.yaml
# Ustaw:
#   manager.endpoint: "192.168.64.1:5443"
#   manager.enrollment_token: "<TOKEN_Z_MACA>"
```

## 6. Uruchomienie

### Mac (terminal 1) — manager dla VM:

```bash
cd ~/Projects/App/ScroogeDLP
make dev-up           # postgres + certy (jeśli jeszcze nie)
make dev-manager-vm   # manager listenuje na 0.0.0.0:5443 i :55000
```

> ⚠️ **Różnica od `make dev-manager`**: VM-tryb używa `manager.dev-vm.yaml`
> z `0.0.0.0` zamiast `127.0.0.1` (inaczej VM nie połączyłaby się). Cert
> ma w SAN `192.168.64.1` (UTM gateway).

### VM (terminal 2) — agent:

```bash
ssh scrooge-vm
cd ~/scrooge-dlp
./deploy/dev/rebuild-and-restart-agent.sh
```

Skrypt:
1. `git pull --ff-only`
2. `cargo build --release -p scrooge-agent-bin` (~3 min pierwszy raz, ~5s rebuilds)
3. `sudo install target/release/scrooge-agent /usr/local/bin/`
4. `sudo systemctl restart scrooge-agent`
5. `sudo journalctl -u scrooge-agent -f` (Ctrl-C żeby przerwać tail)

### Mac (terminal 3) — weryfikacja:

```bash
TOKEN=$(curl -s -X POST http://127.0.0.1:55000/api/v1/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin123"}' | jq -r .access_token)

curl -s -H "Authorization: Bearer $TOKEN" http://127.0.0.1:55000/api/v1/agents | jq

# Oczekiwany output: 2 agentów — Mac (jeśli leci `make dev-agent`) + Ubuntu VM.
```

## 7. Daily workflow

```
┌─ Mac ──────────────────────────────────────┐    ┌─ Ubuntu VM ────────────────┐
│                                            │    │                            │
│  edytujesz kod                             │    │  ssh scrooge-vm            │
│         │                                  │    │         │                  │
│         ▼                                  │    │         ▼                  │
│  git commit + push                         │───▶│  rebuild-and-restart-      │
│                                            │    │     agent.sh               │
│                                            │    │  (git pull → build →       │
│  (jeśli zmieniłeś manager/proto)           │    │   restart → tail logs)     │
│  Ctrl-C w terminalu z managerem            │    │                            │
│  make dev-manager-vm                       │    │  systemd auto-restart      │
│                                            │    │  jeśli proces padnie       │
└────────────────────────────────────────────┘    └────────────────────────────┘
```

## 8. Troubleshooting

### „connection refused" / „failed to connect" w logach agenta

- Sprawdź czy manager leci na Macu (`lsof -nP -i :5443` powinien pokazać `scrooge-manager`).
- Czy używasz `make dev-manager-vm` (nie `make dev-manager` — ten słucha tylko na `127.0.0.1`).
- Czy VM widzi Maca: `ping 192.168.64.1` (powinno odpowiadać).
- Czy ufw na Macu nie blokuje (UTM Shared Network jest sub-interfejsem, zwykle nie ma ufw na Macu).

### TLS verification failed

- Cert managera nie ma IP Maca w SAN. Wygeneruj świeży: `rm dev-certs/server.{pem,key} && make dev-certs`.
- Jeśli Mac ma inny IP w UTM niż domyślne `192.168.64.1`, przekaż przez env:
  `UTM_MAC_IP=192.168.65.1 bash deploy/dev/generate-dev-certs.sh`.

### „token expired" / „invalid enrollment token"

- Token domyślnie ważny 30 dni. Wygeneruj świeży: `MANAGER_CONFIG=deploy/dev/manager.dev.yaml ./target/debug/scroogectl gen-token`.

### Agent loguje się raz, potem zostaje "Stream disconnected"

- To może być zbyt agresywne firewalle (nic na Macu by tego nie powinno wywołać).
- Sprawdź `sudo journalctl -u scrooge-agent --since "5 min ago"` na VM — może być panic lub OOM (zwiększ RAM VM).

### VM ma IP inne niż 192.168.64.x

- UTM "Shared Network" używa zakresu konfigurowalnego w UTM → Preferences → Networking. Default to `192.168.64.0/24`, ale czasem konflikt z VPN — UTM wybiera inny zakres.
- Sprawdź IP gateway'a w VM: `ip route | grep default` → IP managera dla agenta.

### Build na VM długo trwa

- Pierwszy build trwa 3–5 min (compile wszystkich dep'ów).
- Kolejne builds ~5–30s (incremental).
- Można też **cross-compile z Maca**:
  ```bash
  cargo install cross --locked
  make dev-build-agent-linux        # buduje na docelowy linux x86_64/arm64
  scp target/x86_64-unknown-linux-gnu/release/scrooge-agent scrooge-vm:~/
  ```
  ale wymaga Docker na Macu i nie zawsze idzie gładko z dep'ami systemowymi.

---

## Skróty komend

| Komenda na Macu | Co robi |
|---|---|
| `make dev-up` | Postgres w kontenerze + dev CA + server cert |
| `make dev-bootstrap` | Migracje DB + admin + enrollment token |
| `make dev-manager` | Manager natywnie, listen `127.0.0.1` (tylko Mac) |
| `make dev-manager-vm` | Manager natywnie, listen `0.0.0.0` (VM też się połączy) |
| `make dev-agent` | Agent natywnie na Macu (do testów Mac-only) |
| `make dev-agent-reset` | Wyczyść `agent-data/` (force re-enrollment) |
| `make dev-down` | Zatrzymaj Postgres (volume z danymi zostaje) |

| Komenda na VM | Co robi |
|---|---|
| `./deploy/dev/setup-ubuntu-vm.sh` | One-time setup (Rust, Docker, systemd, ufw) |
| `./deploy/dev/rebuild-and-restart-agent.sh` | Daily: pull → build → restart → tail |
| `sudo systemctl status scrooge-agent` | Status agenta |
| `sudo journalctl -u scrooge-agent -f` | Live logi |
| `sudo systemctl stop scrooge-agent` | Zatrzymanie (do debugowania) |

#!/bin/bash

# 🔧 Set your service name here (must match the binary file name)
SERVICE_NAME="mongodb-utils"

# 📁 Current directory used for WorkingDirectory and logs
CURRENT_DIR="$(pwd)"
EXECUTABLE="${CURRENT_DIR}/${SERVICE_NAME}"
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"

# ❗ Check if executable exists
if [ ! -f "$EXECUTABLE" ]; then
  echo "❌ Error: Executable '$EXECUTABLE' not found in $CURRENT_DIR."
  exit 1
fi

# ✅ Make executable
chmod +x "$EXECUTABLE"

# 📝 Generate systemd service file
cat <<EOF | sudo tee "$SERVICE_FILE" > /dev/null
[Unit]
Description=${SERVICE_NAME^} Service
After=network.target

[Service]
User=root
Group=root
Restart=always
RestartSec=5s
StandardOutput=append:${CURRENT_DIR}/std.log
StandardError=append:${CURRENT_DIR}/std_error.log
WorkingDirectory=${CURRENT_DIR}
ExecStart=${EXECUTABLE}

[Install]
WantedBy=multi-user.target
EOF

# 🔍 Check if service is already running and stop it
if sudo systemctl is-active --quiet "$SERVICE_NAME"; then
  echo "⚠️  Service '$SERVICE_NAME' is already running. Stopping it..."
  sudo systemctl stop "$SERVICE_NAME"
  echo "✅ Service stopped successfully"
fi

# 🔍 Check if service exists and disable it
if sudo systemctl list-unit-files | grep -q "^${SERVICE_NAME}.service"; then
  echo "⚠️  Disabling existing service..."
  sudo systemctl disable "$SERVICE_NAME" 2>/dev/null || true
fi

# 🔄 Reload and manage service
echo "🔄 Reloading systemd daemon..."
sudo systemctl daemon-reload

echo "✅ Enabling service..."
sudo systemctl enable "$SERVICE_NAME"

echo "🚀 Starting service..."
sudo systemctl start "$SERVICE_NAME"

# 📊 Show service status
echo ""
echo "📊 Service Status:"
sudo systemctl status "$SERVICE_NAME" --no-pager

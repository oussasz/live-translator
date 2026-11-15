#!/bin/sh
# Let the input group use /dev/uinput (virtual keyboard) and load the module now and at boot.
modprobe uinput 2>/dev/null || true
udevadm control --reload-rules 2>/dev/null || true
udevadm trigger --subsystem-match=misc --action=add 2>/dev/null || true
echo "Live Translator: add yourself to the input group once, then log out and back in:"
echo "  sudo usermod -aG input \$USER"
exit 0

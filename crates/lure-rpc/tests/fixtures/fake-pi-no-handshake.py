#!/usr/bin/env python3
import signal
import sys
import time

if "--version" in sys.argv:
    print("0.0.0-test")
    raise SystemExit(0)

signal.signal(signal.SIGTERM, signal.SIG_IGN)

for _line in sys.stdin:
    time.sleep(10)

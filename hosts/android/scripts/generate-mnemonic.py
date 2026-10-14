#!/usr/bin/env python3

import subprocess
import sys

try:
    from mnemonic import Mnemonic
except ImportError:
    subprocess.check_call([sys.executable, "-m", "pip", "install", "--break-system-packages", "mnemonic", "-q"])
    from mnemonic import Mnemonic

m = Mnemonic('english')
words = m.generate(strength=128)
print(words)

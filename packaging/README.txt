SetMTU {VERSION}
================

Sets a network interface's MTU (1428 bytes by default) for connections that
can't carry the Windows default of 1500 bytes, and restores the original
value with one click.

Running it
----------
1. Extract SetMTU.exe anywhere. There is no installer.
2. Run SetMTU.exe.
3. If Windows shows "Windows protected your PC", click "More info", then
   "Run anyway". The app isn't code-signed yet.
4. Pick the interface and MTU, then click Apply. Windows asks for
   administrator approval (UAC) for each change.
5. "Restore original" puts back the values you had before.

Settings and saved originals are stored in %LOCALAPPDATA%\SetMTU\.

Requirements: Windows 10 or 11, 64-bit.

Source code, issues and new versions:
https://github.com/riaanjutte/SetMTU

MIT licence (see LICENSE.txt). Includes the Inter typeface under the
SIL Open Font License 1.1 (see Inter-OFL.txt).

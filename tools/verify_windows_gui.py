"""Reject Windows client artifacts that could open an unwanted console window.

Check the linked executable, not a Cargo profile or source-code convention. This is a
mandatory packaging gate for both testing and shipping builds. Standard library only,
so the same verification can also be run on a downloaded executable on any host.
"""

import argparse
from pathlib import Path
import struct


WINDOWS_GUI = 2
AMD64 = 0x8664
PE32_PLUS = 0x20B


def read_exact(stream, length):
    data = stream.read(length)
    if len(data) != length:
        raise ValueError("truncated PE executable")
    return data


def verify_windows_gui(executable):
    with Path(executable).open("rb") as stream:
        dos = read_exact(stream, 64)
        if dos[:2] != b"MZ":
            raise ValueError("missing DOS executable signature")
        pe_offset = struct.unpack_from("<I", dos, 0x3C)[0]
        if pe_offset < len(dos):
            raise ValueError("invalid PE header offset")
        stream.seek(pe_offset)
        if read_exact(stream, 4) != b"PE\0\0":
            raise ValueError("missing PE executable signature")
        coff = read_exact(stream, 20)
        if struct.unpack_from("<H", coff)[0] != AMD64:
            raise ValueError("expected a Windows x64 executable")
        characteristics = struct.unpack_from("<H", coff, 18)[0]
        if not characteristics & 0x0002 or characteristics & 0x2000:
            raise ValueError("expected an executable image, not a DLL")
        optional_size = struct.unpack_from("<H", coff, 16)[0]
        if optional_size < 70:
            raise ValueError("PE optional header is too short to contain the subsystem")
        optional = read_exact(stream, optional_size)
        if struct.unpack_from("<H", optional)[0] != PE32_PLUS:
            raise ValueError("expected a PE32+ executable")
        # Subsystem is at offset 68 of the PE optional header, not the COFF header.
        subsystem = struct.unpack_from("<H", optional, 68)[0]
        if subsystem != WINDOWS_GUI:
            raise ValueError(
                f"Windows subsystem must be GUI (2), found {subsystem}; "
                "refusing to package a client that could open a console window"
            )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    args = parser.parse_args()
    try:
        verify_windows_gui(args.executable)
    except (OSError, ValueError) as error:
        parser.exit(1, f"{args.executable}: {error}\n")
    print(f"{args.executable}: verified Windows x64 GUI subsystem (no automatic console)")


if __name__ == "__main__":
    main()

"""Regression tests for the Windows client packaging gate (no Windows host required)."""

from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest

from verify_windows_gui import verify_windows_gui


def pe_image(subsystem=2, machine=0x8664, magic=0x20B):
    image = bytearray(128 + 24 + 240)
    image[:2] = b"MZ"
    struct.pack_into("<I", image, 0x3C, 128)
    image[128:132] = b"PE\0\0"
    struct.pack_into("<H", image, 132, machine)
    struct.pack_into("<H", image, 132 + 16, 240)
    struct.pack_into("<H", image, 132 + 18, 0x0022)
    struct.pack_into("<H", image, 152, magic)
    struct.pack_into("<H", image, 152 + 68, subsystem)
    return image


class VerifyWindowsGuiTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.executable = Path(self.directory.name) / "concord.exe"

    def verify(self, image):
        self.executable.write_bytes(image)
        verify_windows_gui(self.executable)

    def test_accepts_x64_gui(self):
        self.verify(pe_image())

    def test_rejects_console_regardless_of_build_profile(self):
        with self.assertRaisesRegex(ValueError, "found 3.*console window"):
            self.verify(pe_image(subsystem=3))

    def test_rejects_other_subsystems(self):
        for subsystem in (0, 1, 7, 10):
            with self.subTest(subsystem=subsystem):
                with self.assertRaisesRegex(ValueError, "must be GUI"):
                    self.verify(pe_image(subsystem=subsystem))

    def test_rejects_bad_signatures(self):
        for offset in (0, 128):
            image = pe_image()
            image[offset] = 0
            with self.subTest(offset=offset):
                with self.assertRaisesRegex(ValueError, "signature"):
                    self.verify(image)

    def test_rejects_invalid_pe_offset(self):
        image = pe_image()
        struct.pack_into("<I", image, 0x3C, 32)
        with self.assertRaisesRegex(ValueError, "header offset"):
            self.verify(image)

    def test_rejects_truncated_headers(self):
        for length in (0, 63, 130, 140, 221, 391):
            with self.subTest(length=length):
                with self.assertRaisesRegex(ValueError, "truncated"):
                    self.verify(pe_image()[:length])

    def test_rejects_short_optional_header(self):
        image = pe_image()
        struct.pack_into("<H", image, 132 + 16, 68)
        with self.assertRaisesRegex(ValueError, "too short"):
            self.verify(image)

    def test_rejects_wrong_architecture_or_format(self):
        with self.assertRaisesRegex(ValueError, "x64"):
            self.verify(pe_image(machine=0x14C))
        with self.assertRaisesRegex(ValueError, "PE32\\+"):
            self.verify(pe_image(magic=0x10B))

    def test_rejects_a_dll(self):
        image = pe_image()
        struct.pack_into("<H", image, 132 + 18, 0x2022)
        with self.assertRaisesRegex(ValueError, "not a DLL"):
            self.verify(image)

    def test_command_line_blocks_console_artifacts(self):
        self.executable.write_bytes(pe_image(subsystem=3))
        command = [sys.executable, str(Path(__file__).with_name("verify_windows_gui.py")),
                   str(self.executable)]
        result = subprocess.run(command, capture_output=True, text=True, check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("refusing to package", result.stderr)
        self.executable.write_bytes(pe_image())
        result = subprocess.run(command, capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("verified Windows x64 GUI", result.stdout)


if __name__ == "__main__":
    unittest.main()

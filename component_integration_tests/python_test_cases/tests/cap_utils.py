"""
Module for executable capabilities handling.
"""

__all__ = ["get_caps", "set_caps"]

from pathlib import Path
import re
from subprocess import DEVNULL, PIPE, Popen


def get_caps(executable_path: Path | str) -> dict[str, str]:
    """
    Check capabilities of the executable.

    Parameters
    ----------
    executable_path : Path | str
        Resolved path to executable.
        "getcap" is unable to get caps from symlink.
    """
    # Run 'getcap' command.
    command = ["getcap", "-v", executable_path]
    with Popen(command, stdout=PIPE, stderr=DEVNULL, text=True) as p:
        stdout, _ = p.communicate()

    # Split result to lines.
    lines = stdout.strip().split("\n")
    if len(lines) != 1:
        raise RuntimeError("Invalid getcap result")

    # Process line.
    # 'getcap' returns caps grouped by permissions:
    # `<EXECUTABLE_NAME> cap_chown=eip cap_sys_chroot,cap_sys_nice+ep`
    entries = lines[0].split(" ")[1:]
    result = {}
    for entry in entries:
        # Permissions might be after '=' or '+'.
        names, perms = re.split(r"\+|=", entry, 1)

        # Multiple cap names might have same permissions.
        for name in names.split(","):
            result[name] = perms

    return result


def set_caps(executable_path: Path | str, caps: dict[str, str]) -> None:
    """
    Set capabilities of the executable.
    Root permissions are required for this operation.

    Parameters
    ----------
    executable_path : Path | str
        Resolved path to executable.
        "setcap" is unable to grant caps to symlink.
    caps : dict[str, str]
        Capabilities to set.
    """
    caps_list = []
    for name, perms in caps.items():
        caps_list.append(f"{name}+{perms}")
    caps_str = " ".join(caps_list)

    # Run 'setcap' command.
    command = [
        "sudo",
        "setcap",
        caps_str,
        str(executable_path),
    ]
    with Popen(command) as p:
        _, _ = p.communicate()
        if p.returncode != 0:
            raise RuntimeError(f'"setcap" failed with returncode: {p.returncode}')

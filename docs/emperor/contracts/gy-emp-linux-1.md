# GY-EMP-LINUX/1

Contract identifier: `GY-EMP-LINUX/1`. Programme GY-EMP-001, revision 1.0.0. Date 16 November 2026.

This contract is the Linux desktop integration surface. The full surface includes XDG base directories, desktop entries, application ids, MIME associations, icons, file dialogs, URI opening, notifications, settings, startup activation, and portals. Almost all of that is outside EMP-M1.

Repository status: desktop-entry generation without install is specified for this cycle in `crates/emperor`. Portals are not started. Filesystem capability for profile `emperor.linux.interactive/0.0.1` is `none`. Nothing in this contract is a qualified desktop integration.

## Desktop entry emitted for the counter

`build` writes `counter.desktop` in the output directory. It does not copy the file to an XDG applications directory, does not run `update-desktop-database`, and does not install an icon.

The file SHALL contain these keys:

```text
[Desktop Entry]
Type=Application
Name=Count
Exec=App
Terminal=false
Categories=Utility;
```

`Name` is the client text of the admitted counter, which is `Count`. `Exec` is the relative binary name `App` produced in the same directory. It is not an absolute installed path. `Terminal` is false because the programme is a GUI image. `Categories` ends with the semicolon the desktop-entry spec uses for a string list.

Validation for this cycle is that those keys exist. No AppStream metainfo is written. No `desktop-file-validate` run is required for the file to be emitted, and a missing validator is not a pass of the metadata gate. The later programme rule, that generated metadata be checked with a specification tool, stays Unmet.

## Capabilities

The capability manifest for this profile records:

- `filesystem`: `none`
- `network`: false
- `subprocess`: false
- `web_content`: false
- copy and paste as admitted (`true` and `false` on the fixture)
- cut as admitted (`default` on the fixture)

`none` means the profile declares no filesystem scope and no operation class. It does not mean Landlock, a portal, or a mount namespace is in force. The compiler SHALL reject a NIR in this profile that claims any other filesystem value or that sets network, subprocess, or web content true.

There is no user-selected document, no private application-state directory, no temporary scratch directory, and no external subprocess. Open and save dialogs are not generated.

## Blocked integration

The following are Blocked. The compiler must not report them as PASS.

- XDG Desktop Portals, including file chooser, URI open, and notification portals.
- MIME type association and MIME installation.
- Notifications.
- Flatpak, and portal mediation inside a Flatpak sandbox.
- Icon installation at any size.
- AppStream metainfo.
- Startup activation, autostart files, and systemd user units.
- Settings storage under XDG base directories.

`package --format linux-archive` may tar the output directory. That archive is not an installed application. AppImage, Flatpak, deb, and rpm stay Blocked. Installing the archive by hand does not turn the generated desktop file into a completed XDG integration.

A direct native mechanism for a mediated action is not available in this cycle, so the rule that a direct mechanism must be explicitly selected does not grant any extra host access.

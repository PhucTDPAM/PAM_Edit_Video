# Notice

PAM_Edit Video is a modified version of Concat, a video editor licensed under
the GNU Affero General Public License, version 3 or later. Copyright 2026
Jareer and Concat contributors. The original copyright notices are kept in the
source files as the licence requires.

Modifications made for PAM_Edit Video (2026-10-15):

- Renamed the product, crates, binaries, identifiers, installers and packages
  from Concat to PAM_Edit Video / `pamedit`, and replaced the logo and icons.
- Turned off online self-updates (`pamedit-host/src/updates.rs`).
- Removed the upstream model mirror; models are fetched from their original
  publishers and checked against their SHA-256 (`pamedit-host/src/models.rs`).
- Removed upstream-only documents (audits, translated READMEs, CLA, trademark
  and code-signing policies) and upstream links.
- New Windows installer identifiers, so this build installs beside the
  original rather than over it.

PAM_Edit Video is not affiliated with or endorsed by the Concat project.

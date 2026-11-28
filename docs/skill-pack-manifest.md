# Skill Pack manifests

Use **Export with Skill files (ZIP)** to move a local Skill Pack between macOS and Windows. The ZIP contains the Pack's ordered definition, the Skill files in the center library, their fingerprints, and any recommended built-in Agents. Choose **Import Skill Pack** in the destination's local library, open the ZIP, and restore missing members from the preview. For a same-ID Skill with different content, **Restore as a copy** preserves both copies and uses the new ID in the imported Pack.

If a local ID differs from an archived ID only by case, the preview identifies the occupied ID and offers **Restore as a copy**.

On macOS, the preview also compares executable permissions for ZIP imports. If the files match but the permissions differ, you can restore the archived copy under a new ID or explicitly keep the library’s copy. AgentBro rechecks the permissions before creating the Pack.

The preview shows any recorded GitHub repository, Skill path, and commit included in the ZIP. Restoration uses the files in the ZIP offline, including any local edits made after the recorded commit. The restored Skill keeps that source history when you give it a new ID or export it again.

ZIP files must be 128 MiB or smaller, with at most 20,000 files and 256 MiB of uncompressed content. Export includes regular files and in-folder link targets. Git and Python cache directories and links outside the Skill folder are skipped. JSON manifests support the local library and SSH environments.

Use **Export Skill Pack** in a pack’s more-actions menu to save its definition as a JSON file. Commit the file to your own Git repository to review changes to the pack. Exporting the built-in pack captures its current member list.

Use **Import Skill Pack** on the Skill Packs page to open the file. The preview lists the exact Skill IDs in order and compares recorded fingerprints with files in the selected local or SSH center library. Install missing Skills and select **Recheck Skills**. If the library has a different copy or the manifest omits a fingerprint, you can confirm that you want to use the copy in the selected library. Confirming creates a new pack that you can apply to Agents through the usual Skill Packs workflow.

Use **Export with GitHub sources** to check the recorded repository paths and commits against the current library files. The export keeps the current fingerprints and includes only sources that can reproduce them. The result reports how many sources were verified. Use **Export Skill Pack** to save the definition and fingerprints offline.

Use **Export for migration** to also record the built-in Agents that use the pack. The manifest includes their stable IDs. During import, **Create and choose Agents** saves the reviewed pack and opens the normal application dialog with available suggestions selected. Edit the selection, preview the changes, and confirm to write to the selected local or SSH environment.

In the import preview, **Restore missing Skills** restores the missing members that have a recorded source, then compares the library again. You can also restore individual members. If one Skill fails, the preview shows its error and lets you retry it. Select **Create Skill Pack** when the reviewed members are ready.

If a pinned member has different content from the Skill with the same ID in the selected library, choose **Restore as a copy** and confirm a new Skill ID. Both versions stay in the library. The imported Pack uses the new copy at the original member's position and retains its verified fingerprint and source when exported.

```json
{
  "format": "agentbro-skill-pack",
  "version": 2,
  "name": "Team review",
  "description": "Skills for reviewing changes and preparing releases.",
  "tags": ["team", "review"],
  "skillIds": ["code-review", "release-checklist"],
  "skillFingerprints": {
    "code-review": "5026cffe508557c4ea208f5c48792a6b5d38ddcd9326de5c0e238c658979b728"
  }
}
```

Manifests use UTF-8 JSON files up to 1 MiB. The `name` and each Skill ID must be nonempty. Skill IDs are case-sensitive, unique and ordered. Use the ID recorded in the center library. The `description` and `tags` fields may be omitted.

Version 4 can record recommended built-in Agent IDs in `suggestedAgentIds`. Each ID is a unique lowercase identifier of up to 64 ASCII characters, with digits and hyphens allowed. Version 3 can record pinned GitHub sources in `skillSources`. Version 2 records fingerprints as lowercase SHA-256 digests keyed by member ID. Version 1 compares member IDs. Keep the Skill files in their own repositories or another source your team can access.

A portable fingerprint hashes files in sorted order by relative UTF-8 path with forward slashes. For each file it includes the path, a zero byte, the decimal byte length, another zero byte, and the contents. CRLF is normalized to LF in files without NUL bytes. The fingerprint skips Git and Python cache directories and symlinks that leave the Skill folder.

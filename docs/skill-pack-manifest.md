# Skill Pack manifests

Use **Export Skill Pack** in a pack’s more-actions menu to save its definition as a JSON file. Commit the file to your own Git repository to review changes to the pack. Exporting the built-in pack captures its current member list.

Use **Import Skill Pack** on the Skill Packs page to open the file. The preview lists the exact Skill IDs in order and compares recorded fingerprints with files in the selected local or SSH center library. Install missing Skills and select **Recheck Skills**. If the library has a different copy or the manifest omits a fingerprint, you can confirm that you want to use the copy in the selected library. Confirming creates a new pack that you can apply to Agents through the usual Skill Packs workflow.

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

Version 2 records the available fingerprints as lowercase SHA-256 digests keyed by member ID. The importer can still open version 1 files, which compare the member IDs. Keep the Skill files in their own repositories or another source your team can access.

A portable fingerprint hashes files in sorted order by relative UTF-8 path with forward slashes. For each file it includes the path, a zero byte, the decimal byte length, another zero byte, and the contents. CRLF is normalized to LF in files without NUL bytes. The fingerprint skips Git and Python cache directories and symlinks that leave the Skill folder.

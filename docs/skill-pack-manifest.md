# Skill Pack manifests

Use **Export Skill Pack** in a pack’s more-actions menu to save its definition as a JSON file. Commit the file to your own Git repository to review changes to the pack. Exporting the built-in pack captures its current member list.

Use **Import Skill Pack** on the Skill Packs page to open the file. The preview lists the exact Skill IDs in order and highlights any missing from the selected runtime’s center library. Install the missing Skills in the center library and reopen the preview. Confirming creates a new pack that you can apply to Agents through the usual Skill Packs workflow.

```json
{
  "format": "agentbro-skill-pack",
  "version": 1,
  "name": "Team review",
  "description": "Skills for reviewing changes and preparing releases.",
  "tags": ["team", "review"],
  "skillIds": ["code-review", "release-checklist"]
}
```

Version 1 uses UTF-8 JSON files up to 1 MiB. The `name` and each Skill ID must be nonempty. Skill IDs are case-sensitive, unique and ordered. Use the ID recorded in the center library. The `description` and `tags` fields may be omitted.

Keep the Skill files in their own repositories or another source your team can access. The manifest records the pack name, description, tags and member IDs.

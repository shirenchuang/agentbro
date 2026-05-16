cask "agent-island" do
  version "0.1.0"
  sha256 :no_check

  url "https://github.com/agent-island/agent-island/releases/download/v#{version}/AgentIsland.dmg"
  name "Agent Island"
  desc "Menu bar companion for Claude Code, Codex, Gemini CLI and more"
  homepage "https://github.com/agent-island/agent-island"

  app "Agent Island.app"

  zap trash: [
    "~/.agent-island",
    "~/Library/Application Support/com.agent-island.app",
    "~/Library/Preferences/com.agent-island.app.plist",
  ]
end

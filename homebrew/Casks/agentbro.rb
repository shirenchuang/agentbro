cask "agentbro" do
  version "0.1.0"
  sha256 :no_check

  url "https://github.com/agentbro/agentbro/releases/download/v#{version}/AgentBro_#{version}_universal.dmg"
  name "AgentBro"
  desc "Menu bar companion for Claude Code, Codex, Gemini CLI and more"
  homepage "https://github.com/agentbro/agentbro"

  app "AgentBro.app"

  zap trash: [
    "~/.agentbro",
    "~/Library/Application Support/com.agentbro.desktop",
    "~/Library/Preferences/com.agentbro.desktop.plist",
  ]
end

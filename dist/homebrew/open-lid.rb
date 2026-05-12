cask "open-lid" do
  version "0.1.0"
  sha256 "REPLACE_WITH_SHA256_AT_RELEASE_TIME"

  url "https://github.com/diyanbogdanov/open-lid/releases/download/v#{version}/OpenLid-v#{version}.dmg"
  name "Open-Lid"
  desc "Keep your Mac awake — even with the lid closed"
  homepage "https://github.com/diyanbogdanov/open-lid"

  depends_on macos: ">= :ventura"
  depends_on arch: :arm64

  app "OpenLid.app"

  postflight do
    # Helper install requires sudo and can't be done from a cask postflight
    # without user interaction. Print instructions instead.
    ohai "Open-Lid is installed but the privileged helper is not."
    ohai "To enable sleep prevention, run:"
    ohai "  /Applications/OpenLid.app/Contents/Resources/install-helper.sh"
    ohai "(this requires your admin password — only once)."
  end

  zap trash: [
    "~/Library/Application Support/open-lid",
    "~/Library/Application Support/Logs/open-lid",
    "/Library/LaunchDaemons/io.openlid.helper.plist",
    "/Library/Application Support/open-lid",
    "/Library/Logs/open-lid",
  ]
end

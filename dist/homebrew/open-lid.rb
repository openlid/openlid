cask "open-lid" do
  version "1.0.0"
  sha256 "48f7439d15cd2b80c758235523e9630207c19f7a12f8ced5a57d18871f1054e9"

  url "https://github.com/openlid/open-lid/releases/download/v#{version}/OpenLid-v#{version}.dmg",
      verified: "github.com/openlid/open-lid/"
  name "Open-Lid"
  desc "Keep your Mac awake — even with the lid closed"
  homepage "https://github.com/openlid/open-lid"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :ventura"
  depends_on arch: :arm64

  app "OpenLid.app"
  binary "#{appdir}/OpenLid.app/Contents/MacOS/open-lid"

  postflight do
    ohai "Open-Lid installed."
    ohai "Launch from /Applications, `open -a OpenLid`, or run `open-lid` in your terminal."
    ohai "On first launch, macOS will ask you to approve the helper in:"
    ohai "  System Settings → General → Login Items → Allow in the Background"
    ohai "Flip the Open-Lid toggle on — no admin password required."
  end

  uninstall launchctl: "io.openlid.helper",
            quit:      "io.openlid.app",
            delete:    "/Library/LaunchDaemons/io.openlid.helper.plist"

  zap trash: [
    "/Library/Application Support/open-lid",
    "/Library/Logs/open-lid",
    "~/Library/Application Support/io.openlid.open-lid",
    "~/Library/Application Support/Logs/open-lid",
    "~/Library/Logs/open-lid",
  ]
end

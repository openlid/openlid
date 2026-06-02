cask "openlid" do
  version "2.3.1"
  sha256 "cf637de9568c801e1fec920b69bbd72dcb3c5ab53e7780a874e22906c2f00055"

  url "https://github.com/openlid/openlid/releases/download/v#{version}/OpenLid-v#{version}.dmg",
      verified: "github.com/openlid/openlid/"
  name "Open-Lid"
  desc "Keep your Mac awake — even with the lid closed"
  homepage "https://github.com/openlid/openlid"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :ventura"
  depends_on arch: :arm64

  app "OpenLid.app"
  binary "#{appdir}/OpenLid.app/Contents/MacOS/openlid"

  postflight do
    ohai "Open-Lid installed."
    ohai "Launch from /Applications, `open -a OpenLid`, or run `openlid` in your terminal."
    ohai "On first launch, macOS will ask you to approve the helper in:"
    ohai "  System Settings → General → Login Items → Allow in the Background"
    ohai "Flip the Open-Lid toggle on — no admin password required."
  end

  uninstall launchctl: "io.openlid.helper",
            quit:      "io.openlid.app",
            delete:    "/Library/LaunchDaemons/io.openlid.helper.plist"

  zap trash: [
    "/Library/Application Support/openlid",
    "/Library/Application Support/open-lid",
    "/Library/Logs/openlid",
    "/Library/Logs/open-lid",
    "~/Library/Application Support/io.openlid.app",
    "~/Library/Application Support/io.openlid.open-lid",
    "~/Library/Application Support/Logs/openlid",
    "~/Library/Application Support/Logs/open-lid",
    "~/Library/Logs/openlid",
    "~/Library/Logs/open-lid",
  ]
end

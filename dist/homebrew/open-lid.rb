cask "open-lid" do
  version "0.2.0"
  sha256 "3d0b09311ebd1459654393290aa608a3b701ca7ecd81cb91e693e0f7e82b1dfd"

  url "https://github.com/diyanbogdanov/open-lid/releases/download/v#{version}/OpenLid-v#{version}.dmg",
      verified: "github.com/diyanbogdanov/open-lid/"
  name "Open-Lid"
  desc "Keep your Mac awake — even with the lid closed"
  homepage "https://github.com/diyanbogdanov/open-lid"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :ventura"
  depends_on arch: :arm64

  app "OpenLid.app"

  postflight do
    ohai "Open-Lid installed."
    ohai "Launch it from /Applications or with `open -a OpenLid`."
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

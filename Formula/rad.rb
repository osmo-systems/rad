class Rad < Formula
  desc "Terminal radio player powered by RadioBrowser"
  homepage "https://github.com/osmo-systems/radm"
  version "0.1.0"
  license "MIT OR Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/osmo-systems/radm/releases/download/v#{version}/rad-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end

    on_intel do
      url "https://github.com/osmo-systems/radm/releases/download/v#{version}/rad-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    depends_on "alsa-lib"

    on_arm do
      url "https://github.com/osmo-systems/radm/releases/download/v#{version}/rad-#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end

    on_intel do
      url "https://github.com/osmo-systems/radm/releases/download/v#{version}/rad-#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    bin.install "rad"
  end

  test do
    assert_match "rad - Radio Player", shell_output("#{bin}/rad --help")
  end
end

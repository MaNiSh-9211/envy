class Envy < Formula
  desc "Universal environment manager — one config format for every stack"
  homepage "https://github.com/MaNiSh-9211/envy"
  version "0.1.0"

  depends_on arch: [:x86_64, :arm64]

  if OS.mac? && Hardware::CPU.arm?
    url "https://github.com/MaNiSh-9211/envy/releases/download/v#{version}/envy-darwin-arm64"
    sha256 "TODO_REPLACE_AFTER_FIRST_RELEASE"
  elsif OS.mac?
    url "https://github.com/MaNiSh-9211/envy/releases/download/v#{version}/envy-darwin-amd64"
    sha256 "TODO_REPLACE_AFTER_FIRST_RELEASE"
  elsif Hardware::CPU.intel?
    url "https://github.com/MaNiSh-9211/envy/releases/download/v#{version}/envy-linux-amd64"
    sha256 "TODO_REPLACE_AFTER_FIRST_RELEASE"
  else
    url "https://github.com/MaNiSh-9211/envy/releases/download/v#{version}/envy-linux-arm64"
    sha256 "TODO_REPLACE_AFTER_FIRST_RELEASE"
  end

  def install
    bin.install(Dir["*"].first => "envy")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/envy --version")
  end
end

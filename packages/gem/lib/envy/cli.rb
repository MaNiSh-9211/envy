require "fileutils"
require "open-uri"

module Envy
  module CLI
    REPO = "MaNiSh-9211/envy"

    module_function

    def asset
      os = case Gem::Platform.local.os
           when /mingw|mswin|windows/ then "windows"
           when "darwin" then "darwin"
           else "linux"
           end
      cpu = %w[arm64 aarch64].include?(Gem::Platform.local.cpu.to_s) ? "arm64" : "amd64"
      ext = os == "windows" ? ".exe" : ""
      "envy-#{os}-#{cpu}#{ext}"
    end

    def binary_path
      dir = File.join(Gem.user_home, ".envy", "bin")
      FileUtils.mkdir_p(dir)
      File.expand_path(asset, dir)
    end

    def ensure_binary!
      path = binary_path
      return path if File.exist?(path)

      version = ENV["ENVY_VERSION"] || "latest"
      segment = version == "latest" ? "latest/download" : "download/#{version}"
      url = "https://github.com/#{REPO}/releases/#{segment}/#{asset}"

      warn "envy: downloading #{url}"
      body = URI.open(url, "rb", "User-Agent" => "envy-installer", &:read)
      File.binwrite(path, body)
      FileUtils.chmod(0o755, path) unless Gem.win_platform?
      path
    rescue OpenURI::HTTPError, SocketError => e
      abort "envy: download failed (#{e.message})"
    end

    def run(args)
      exe = ensure_binary!
      exec([exe, exe], *args)
    rescue Errno::ENOENT, SystemCallError => e
      warn "envy: failed to launch binary (#{e.message})"
      1
    end
  end
end

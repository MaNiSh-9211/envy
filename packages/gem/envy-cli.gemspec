Gem::Specification.new do |s|
  s.name = "envy-cli"
  s.version = "0.1.0"
  s.summary = "envy — universal environment manager for every stack"
  s.description = "Installs and runs the native envy binary: one typed, validated config format injected into any process."
  s.authors = ["MaNiSh-9211"]
  s.homepage = "https://github.com/MaNiSh-9211/envy"
  s.license = "MIT"
  s.required_ruby_version = ">= 2.7"
  s.metadata = {
    "source_code_uri" => "https://github.com/MaNiSh-9211/envy",
    "bug_tracker_uri" => "https://github.com/MaNiSh-9211/envy/issues"
  }
  s.files = Dir["lib/**/*.rb"]
  s.bindir = "bin"
  s.executables = ["envy"]
end

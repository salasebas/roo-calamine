# frozen_string_literal: true

require "fileutils"

# roo's tests and fixtures are not in the roo gem, so the parity run and the
# fixture tests read them from a checkout of the roo 4.0.0 tag.
module RooCheckout
  TAG = "v4.0.0"
  SHA = "a25a8cf97e048ab1e152e5d5a70701eaa518a6bf"
  DIR = File.expand_path("../../tmp/roo-#{TAG}", __dir__)

  def self.ensure!
    unless File.directory?(DIR)
      FileUtils.mkdir_p(File.dirname(DIR))
      system("git", "-c", "advice.detachedHead=false", "clone", "--quiet", "--depth", "1", "--branch", TAG,
             "https://github.com/roo-rb/roo.git", DIR, exception: true)
    end
    sha = IO.popen(["git", "-C", DIR, "rev-parse", "HEAD"], &:read).strip
    raise "#{DIR} is at #{sha}, expected #{SHA}" unless sha == SHA

    DIR
  end
end

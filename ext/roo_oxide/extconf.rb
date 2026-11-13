# frozen_string_literal: true

require "mkmf"

# JRuby cannot load a native extension. Write a no-op Makefile so the gem
# installs, and lib/roo/oxide.rb stays inert.
if RUBY_ENGINE != "ruby"
  File.write("Makefile", "all install clean:\n\t@:\n")
  return
end

require "rb_sys/mkmf"

create_rust_makefile("roo/oxide/roo_oxide")

# frozen_string_literal: true

require "roo"
require "roo/excelx"
require_relative "calamine/version"

module Roo
  # Reads `.xlsx` and `.xlsm` worksheets for Roo::Excelx with a native
  # extension. Loading this file is all it takes; `Roo::Calamine.enabled =
  # false` or `ROO_CALAMINE=0` switches back to roo's own reader at runtime.
  module Calamine
    # Raised by the native reader for XML it does not reproduce exactly the
    # way roo reads it. Roo::Calamine rescues it and lets roo read the sheet.
    class UnsupportedError < StandardError; end

    DISABLING_ENV_VALUES = %w[0 false off].freeze
    private_constant :DISABLING_ENV_VALUES

    @enabled = true

    class << self
      attr_writer :enabled

      # Whether the native extension is loaded. It is `false` on Ruby
      # implementations other than CRuby, such as JRuby.
      def available?
        defined?(Native::Reader) ? true : false
      end

      # Whether the next worksheet read goes through the native reader.
      def enabled?
        @enabled && available? && !DISABLING_ENV_VALUES.include?(ENV.fetch("ROO_CALAMINE", "").downcase)
      end
    end
  end
end

if RUBY_ENGINE == "ruby"
  begin
    require "roo/calamine/#{RUBY_VERSION[/\d+\.\d+/]}/roo_calamine"
  rescue LoadError
    require "roo/calamine/roo_calamine"
  end
  require_relative "calamine/context"
  require_relative "calamine/sheet_doc"
  Roo::Excelx::SheetDoc.prepend(Roo::Calamine::SheetDoc)
end

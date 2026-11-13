# frozen_string_literal: true

module Roo
  module Turbo
    # Prepended to Roo::Excelx::SharedStrings: the native extension builds the
    # arrays #to_a and #to_html memoize, deferring HTML objects until used.
    # Roo's own code and the native reader share them. A file the extension
    # does not read exactly the way roo does is left to roo.
    module SharedStrings
      # roo versions whose shared-string extraction the extension reproduces.
      ROO_VERSIONS = %w[4.0.0].freeze

      def to_a
        turbo_read(html: !@options[:disable_html_wrapper] && !(@html || @turbo_html), plain: true) unless @array
        super
      end

      def to_html
        unless @html
          turbo_read(html: true, plain: false) unless @turbo_html
          @html = @turbo_html.to_a if @turbo_html && (@turbo_html_used || doc_exists?)
        end
        super
      end

      def turbo_html
        return to_html unless @turbo_html && (@turbo_html_used || doc_exists?)

        @turbo_html_used = true
        @html || @turbo_html
      end

      private

      def turbo_read(html:, plain:)
        return unless Turbo.enabled? && ROO_VERSIONS.include?(Roo::VERSION) && doc_exists?

        plain, html = Native.shared_strings(@path, html, plain)
        @array ||= plain
        @turbo_html ||= html
      rescue UnsupportedError
        nil
      end
    end
  end
end

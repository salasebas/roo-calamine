# frozen_string_literal: true

module Roo
  module Oxide
    # Prepended to Roo::Excelx::SharedStrings: the native extension builds the
    # arrays #to_a and #to_html memoize, deferring HTML objects until used.
    # Roo's own code and the native reader share them. A file the extension
    # does not read exactly the way roo does is left to roo.
    module SharedStrings
      # roo versions whose shared-string extraction the extension reproduces.
      ROO_VERSIONS = %w[4.0.0].freeze

      def to_a
        oxide_read(html: !@options[:disable_html_wrapper] && !(@html || @oxide_html), plain: true) unless @array
        super
      end

      def to_html
        unless @html
          oxide_read(html: true, plain: false) unless @oxide_html
          @html = @oxide_html.to_a if @oxide_html && (@oxide_html_used || doc_exists?)
        end
        super
      end

      def oxide_html
        return to_html unless @oxide_html && (@oxide_html_used || doc_exists?)

        @oxide_html_used = true
        @html || @oxide_html
      end

      private

      def oxide_read(html:, plain:)
        return unless Oxide.enabled? && ROO_VERSIONS.include?(Roo::VERSION) && doc_exists?

        plain, html = Native.shared_strings(@path, html, plain)
        @array ||= plain
        @oxide_html ||= html
      rescue UnsupportedError
        nil
      end
    end
  end
end

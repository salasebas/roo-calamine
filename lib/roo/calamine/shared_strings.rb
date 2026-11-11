# frozen_string_literal: true

module Roo
  module Calamine
    # Prepended to Roo::Excelx::SharedStrings: the native extension builds the
    # arrays #to_a and #to_html memoize, deferring HTML objects until used.
    # Roo's own code and the native reader share them. A file the extension
    # does not read exactly the way roo does is left to roo.
    module SharedStrings
      # roo versions whose shared-string extraction the extension reproduces.
      ROO_VERSIONS = %w[4.0.0].freeze

      def to_a
        calamine_read(html: !@options[:disable_html_wrapper] && !(@html || @calamine_html), plain: true) unless @array
        super
      end

      def to_html
        unless @html
          calamine_read(html: true, plain: false) unless @calamine_html
          @html = @calamine_html.to_a if @calamine_html && (@calamine_html_used || doc_exists?)
        end
        super
      end

      def calamine_html
        return to_html unless @calamine_html && (@calamine_html_used || doc_exists?)

        @calamine_html_used = true
        @html || @calamine_html
      end

      private

      def calamine_read(html:, plain:)
        return unless Calamine.enabled? && ROO_VERSIONS.include?(Roo::VERSION) && doc_exists?

        plain, html = Native.shared_strings(@path, html, plain)
        @array ||= plain
        @calamine_html ||= html
      rescue UnsupportedError
        nil
      end
    end
  end
end

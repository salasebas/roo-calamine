# frozen_string_literal: true

require_relative "../turbo/version"

Roo::Calamine = Roo::Turbo unless Roo.const_defined?(:Calamine, false)

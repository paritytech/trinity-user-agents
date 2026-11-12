# frozen_string_literal: true

source "https://rubygems.org"

ruby "3.3.12"

gem "fastlane", ">= 2.240.0"

plugins_path = File.join(File.dirname(__FILE__), 'fastlane', 'Pluginfile')
eval_gemfile(plugins_path) if File.exist?(plugins_path)

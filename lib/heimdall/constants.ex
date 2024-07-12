defmodule Heimdall.Constants do
  @moduledoc """
  Global constants
  """
  @edns_server_cookie 0xDEADBEEF2BAD2DAD
  def server_cookie(), do: @edns_server_cookie
end

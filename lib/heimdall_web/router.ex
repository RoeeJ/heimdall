defmodule HeimdallWeb.Router do
  use HeimdallWeb, :router

  pipeline :browser do
    plug :accepts, ["html"]
    plug :fetch_session
    plug :fetch_live_flash
    plug :put_root_layout, html: {HeimdallWeb.Layouts, :root}
    plug :protect_from_forgery
    plug :put_secure_browser_headers
  end

  pipeline :api do
    plug :accepts, ["json"]
  end

  scope "/", HeimdallWeb do
    pipe_through :browser

    get "/", PageController, :home
    live "/dashboard", DashboardLive
    live "/zones", Zones.IndexLive
    live "/zones/:zone_id", Zones.IdLive
    live "/analytics", Analytics.IndexLive
    live "/blocklists", Blocklists.IndexLive
    live "/settings", SettingsLive
  end

  # Other scopes may use custom stacks.
  scope "/api", HeimdallWeb do
    pipe_through :api

    scope "/cache" do
      get "/clear", CacheController, :clear
      get "/stats", CacheController, :stats
      get "/view", CacheController, :view
    end
  end

  # Enable LiveDashboard and Swoosh mailbox preview in development
  if Application.compile_env(:heimdall, :dev_routes) do
    # If you want to use the LiveDashboard in production, you should put
    # it behind authentication and allow only admins to access it.
    # If your application does not have an admins-only section yet,
    # you can use Plug.BasicAuth to set up some basic authentication
    # as long as you are also using SSL (which you should anyway).
    import Phoenix.LiveDashboard.Router

    scope "/dev" do
      pipe_through :browser

      live_dashboard "/dashboard", metrics: HeimdallWeb.Telemetry
      forward "/mailbox", Plug.Swoosh.MailboxPreview
    end
  end
end

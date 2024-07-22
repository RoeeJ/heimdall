defmodule HeimdallWeb.Zones.IndexLive do
  alias Heimdall.Schema.Zone
  use HeimdallWeb, :live_view

  @impl true
  def mount(_params, _session, socket) do
    socket = socket |> assign(:zones, Zone.all()) |> assign(:selected_zone, nil)
    {:ok, socket}
  end

  @impl true
  def handle_event("select_zone", %{"id" => id}, socket) do
    socket = socket |> push_navigate(to: ~p"/zones/#{id}")
    {:noreply, socket}
  end

  def handle_event("delete_zone", %{"id" => id}, socket) do
    Zone.delete(id)
    {:noreply, socket}
  end
end

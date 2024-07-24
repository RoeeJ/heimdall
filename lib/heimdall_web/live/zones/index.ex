defmodule HeimdallWeb.Zones.IndexLive do
  alias Heimdall.Schema.Zone
  use HeimdallWeb, :live_view

  @impl true
  def mount(_params, _session, socket) do
    socket = socket |> assign(:zones, Zone.all()) |> assign(:selected_zone, nil) |> assign(:zone_name_invalid, true)
    {:ok, socket}
  end

  def handle_event("validate_zone_name", %{"zone" => %{"name" => name}}, socket) do
    is_empty = String.trim(name) == ""
    is_valid_dns = String.match?(name, ~r/^(?!-)[A-Za-z0-9-]{1,63}(?<!-)(\.[A-Za-z0-9-]{1,63})*$/)
    is_taken = Zone.exists?(name)
    socket = socket |> assign(:zone_name_invalid, is_taken or is_empty or not is_valid_dns)
    {:noreply, socket}
  end

  @impl true
  def handle_event("create_zone", %{"zone" => %{"name" => name}}, socket) do
    case Zone.create(name) do
      {:ok, zone} ->
        {:noreply, socket |> assign(:zones, Zone.all()) |> assign(:selected_zone, zone)}

      {:error, changeset} ->
        errors =
          changeset.errors
          |> Enum.map_join(", ", fn {field, {msg, _}} -> "#{field}: #{msg}" end)

        socket = socket |> put_flash(:error, "Failed to create zone: #{errors}")
        {:noreply, socket}
    end
  end

  @impl true
  def handle_event("edit_zone", %{"id" => id}, socket) do
    socket = socket |> push_navigate(to: ~p"/zones/#{id}")
    {:noreply, socket}
  end

  def handle_event("delete_zone", %{"id" => id}, socket) do
    case Enum.find(socket.assigns.zones, fn zone -> zone.id == String.to_integer(id) end) do
      nil ->
        {:noreply, socket}

      zone ->
        Zone.delete(zone)
        socket = socket |> assign(:zones, Zone.all())
        {:noreply, socket}
    end
  end
end

defmodule HeimdallWeb.Zones.IdLive do
  use HeimdallWeb, :live_view
  use HeimdallWeb, :verified_routes
  alias Heimdall.DNS.Manager

  defp supported_record_types, do: [:a, :ns, :cname, :hinfo, :mx, :txt, :aaaa, :loc, :srv]

  def mount(%{"zone_id" => zone_id}, _, socket) do
    zone_id = String.to_integer(zone_id)

    socket =
      socket
      |> assign(zone_id: zone_id, create_record: false)
      |> reset_new_record()
      |> refresh_zone()

    {:ok, socket}
  end

  defp convert_params(params) do
    Enum.into(params, %{}, fn {k, v} ->
      key = String.to_existing_atom(k)

      value =
        case key do
          :type -> String.to_existing_atom(v)
          :ttl -> String.to_integer(v)
          _ -> v
        end

      {key, value}
    end)
  end

  def handle_event("update_record", %{"id" => id, "record" => record_params}, socket) do
    updated_records =
      Enum.map(socket.assigns.zone.records, fn record ->
        if record.id == String.to_integer(id) do
          Map.merge(record, convert_params(record_params))
        else
          record
        end
      end)

    {:noreply, assign(socket, :zone, %{socket.assigns.zone | records: updated_records})}
  end

  def handle_event("save_record", %{"id" => id}, socket) do
    record =
      Enum.find(socket.assigns.zone.records, fn record -> record.id == String.to_integer(id) end)

    case Manager.update_record(record.id, Map.from_struct(record)) do
      {:ok, _record} ->
        socket = socket |> refresh_zone()
        {:noreply, socket}

      {:error, _changeset} ->
        {:noreply, socket}
    end
  end

  def handle_event("delete_record", %{"id" => id}, socket) do
    case Manager.delete_record(String.to_integer(id)) do
      {:ok, _record} ->
        socket = socket |> refresh_zone()
        {:noreply, socket}

      {:error, _reason} ->
        {:noreply, socket}
    end
  end

  def handle_event("update_new_record", params, socket) do
    updated_record =
      Enum.reduce(params["record"], socket.assigns.new_record, fn {key, value}, acc ->
        case key do
          "name" -> %{acc | name: value}
          "type" -> %{acc | type: String.to_existing_atom(value)}
          "ttl" -> %{acc | ttl: String.to_integer(value)}
          "data" -> %{acc | data: update_record_data(acc.type, value)}
          _ -> acc
        end
      end)

    {:noreply, assign(socket, :new_record, updated_record)}
  end

  def handle_event("add_record", params, socket) do
    updated_record =
      Enum.reduce(params["record"], %{}, fn {key, value}, acc ->
        case key do
          "name" ->
            Map.put(acc, :name, value)

          "type" ->
            Map.put(acc, :type, String.to_existing_atom(value))

          "ttl" ->
            Map.put(acc, :ttl, String.to_integer(value))

          "data" ->
            Map.put(
              acc,
              :data,
              update_record_data(String.to_existing_atom(params["record"]["type"]), value)
            )

          _ ->
            acc
        end
      end)

    params = Map.put(params, "record", updated_record)

    case Manager.add_record(socket.assigns.zone_id, params["record"]) do
      {:ok, _record} ->
        socket = socket |> reset_new_record() |> refresh_zone()
        {:noreply, socket}

      {:error, _changeset} ->
        {:noreply, socket}
    end
  end

  def handle_event("open_create_dialog", _, socket), do:
    {:noreply, assign(socket, :create_record, true)}

  def handle_event("close_create_dialog", _, socket), do:
    {:noreply, assign(socket, :create_record, false)}

  defp update_record_data(type, data) do
    case type do
      :a -> Map.take(data, ["ip"])
      :cname -> Map.take(data, ["host"])
      :ns -> Map.take(data, ["host"])
      :txt -> Map.take(data, ["text"])
      :mx -> Map.take(data, ["preference", "host"])
      _ -> data
    end
  end

  defp ttl_dropdown(assigns) do
    ~H"""
    <select
      name={@name}
      value={@value}
      phx-value-field={@field}
      required
      class="select select-bordered w-full max-w-xs"
    >
      <option value="60" selected={60 == @value}>1 minute</option>

      <option value="300" selected={300 == @value}>5 minutes</option>

      <option value="600" selected={600 == @value}>10 minutes</option>

      <option value="900" selected={900 == @value}>15 minutes</option>

      <option value="1800" selected={1800 == @value}>30 minutes</option>

      <option value="3600" selected={3600 == @value}>1 hour</option>

      <option value="7200" selected={7200 == @value}>2 hours</option>

      <option value="14400" selected={14400 == @value}>4 hours</option>

      <option value="28800" selected={28800 == @value}>8 hours</option>

      <option value="57600" selected={57600 == @value}>16 hours</option>

      <option value="86400" selected={86400 == @value}>1 day</option>
    </select>
    """
  end

  @spec refresh_zone(Phoenix.LiveView.Socket.t()) :: Phoenix.LiveView.Socket.t()
  defp refresh_zone(socket) do
    case Manager.get_zone(socket.assigns.zone_id) do
      {:ok, zone} ->
        socket
        |> assign(:zone, zone)

      {:error, :zone_not_found} ->
        socket
    end
  end

  @spec reset_new_record(Phoenix.LiveView.Socket.t()) :: Phoenix.LiveView.Socket.t()
  defp reset_new_record(socket) do
    socket
    |> assign(:new_record, %Heimdall.Schema.Record{ttl: 3600, type: :a})
  end
end

defmodule HeimdallWeb.ZonesLive do
  alias Heimdall.Schema.Record
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

  @impl true
  def render(assigns) do
    ~H"""
    <div class="container mx-auto px-4 py-8">
      <h1 class="text-3xl font-bold mb-6 text-gray-800">DNS Zones</h1>

      <div class="bg-white shadow-md rounded-lg overflow-hidden">
        <table class="min-w-full divide-y divide-gray-200">
          <thead class="bg-gray-50">
            <tr>
              <th
                scope="col"
                class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider"
              >
                Name
              </th>

              <th
                scope="col"
                class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider"
              >
                Records
              </th>

              <th
                scope="col"
                class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider"
              >
                Actions
              </th>
            </tr>
          </thead>

          <tbody class="bg-white divide-y divide-gray-200">
            <%= for zone <- @zones do %>
              <tr class="hover:bg-gray-50 transition-colors duration-200">
                <td class="px-6 py-4 whitespace-nowrap text-sm font-medium text-gray-900">
                  <%= zone.name %>
                </td>

                <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                  <%= length(zone.records) %>
                </td>

                <td class="px-6 py-4 whitespace-nowrap text-sm font-medium">
                  <button
                    phx-click="select_zone"
                    phx-value-id={zone.id}
                    class="text-indigo-600 hover:text-indigo-900 focus:outline-none focus:underline"
                  >
                    View Details
                  </button>
                </td>
              </tr>
            <% end %>
          </tbody>
        </table>
      </div>

      <%= if @selected_zone do %>
        <div class="mt-8 bg-white shadow-md rounded-lg overflow-hidden">
          <div class="px-6 py-4 border-b border-gray-200">
            <h2 class="text-xl font-semibold text-gray-800">
              Zone Details: <%= @selected_zone.name %>
            </h2>
          </div>

          <div class="px-6 py-4">
            <h3 class="text-lg font-medium text-gray-800 mb-2">Records</h3>

            <ul class="space-y-2">
              <%= for record <- @selected_zone.records do %>
                <li class="text-sm text-gray-600">
                  <.record_input record={record} />
                </li>
              <% end %>
            </ul>
          </div>
        </div>
      <% end %>
    </div>
    """
  end

  defp record_input(%{record: %Record{type: :a}} = assigns) do
    ~H"""
    <div class="w-full flex flex-row items-center space-x-4 p-4 bg-gray-100 rounded-md">
      <div class="flex flex-col">
        <label class="text-sm font-medium text-gray-700">Name</label>
        <input class="input" value={@record.name} />
      </div>

      <div class="flex flex-col">
        <label class="text-sm font-medium text-gray-700">IP</label>
        <input class="input" value={@record.data["ip"]} />
      </div>

      <div class="flex flex-col">
        <label class="text-sm font-medium text-gray-700">TTL</label>
        <select class="input">
          <option value="60" selected={@record.ttl == 60}>1 minute</option>

          <option value="300" selected={@record.ttl == 300}>5 minutes</option>

          <option value="600" selected={@record.ttl == 600}>10 minutes</option>

          <option value="3600" selected={@record.ttl == 3600}>1 hour</option>

          <option value="86400" selected={@record.ttl == 86400}>1 day</option>
        </select>
      </div>

      <button class="ml-auto text-indigo-600 hover:text-indigo-900 focus:outline-none focus:underline">
        Save
      </button>
    </div>
    """
  end

  defp record_input(
         %{
           record: %Record{
             type: :mx
           }
         } = assigns
       ) do
    ~H"""
    <div class="flex flex-row">
      <input class="input" value={@record.name} />
      <input class="input" value={@record.data["host"]} /><input
        class="input"
        value={@record.data["priority"]}
      /> <input class="input" value={@record.ttl} />
      <.button phx-click="save_record">Save</.button>
    </div>
    """
  end

end

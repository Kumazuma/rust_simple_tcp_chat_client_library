# rust_simple_tcp_chat_client_library
very simple tcp chat client library
# example
```cpp
#include <iostream>
#include <Windows.h>
#include "rust_ffi_interface.h"
#pragma comment(lib, "tokio_client_test_2.dll.lib")
void OnMessage(Client* client, char const* msg, void*)
{
    std::cout << msg << std::endl;
}
int main()
{
    Client* client = Client_New("localhost:8080");
    Client_OnMessage(client, &OnMessage, nullptr);
    for (int i = 0; i < 10000; ++i)
    {
        Client_SendMessage(client, "Hello");
        Sleep(10);
    }
    getchar();
    Client_Delete(client);
    return 0;
}
```

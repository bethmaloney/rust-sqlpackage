CREATE PROCEDURE [dbo].[GetOrdersByStatus]
    @Status INT
AS
BEGIN
    SET NOCOUNT ON;

    DECLARE @FilteredOrders TABLE (
        [OrderId] UNIQUEIDENTIFIER NOT NULL,
        [CustomerId] UNIQUEIDENTIFIER NOT NULL
    )

    INSERT INTO @FilteredOrders
    SELECT [OrderId], [CustomerId] FROM [dbo].[Orders] WHERE [Status] = @Status

    SELECT
        [o].[OrderId],
        [c].[CustomerName]
    FROM
        @FilteredOrders [o]
        INNER JOIN [dbo].[Customers] [c] ON [o].[CustomerId] = [c].[Id]
END
GO

CREATE PROCEDURE [Sales].[CreateOrder]
    @CustomerId INT,
    @OrderNumber NVARCHAR(50),
    @TotalAmount DECIMAL(18, 2),
    @OrderId INT OUTPUT
AS
BEGIN
    SET NOCOUNT ON;

    INSERT INTO [Sales].[Orders] ([OrderNumber], [CustomerId], [TotalAmount])
    VALUES (@OrderNumber, @CustomerId, @TotalAmount);

    SET @OrderId = SCOPE_IDENTITY();
END
GO

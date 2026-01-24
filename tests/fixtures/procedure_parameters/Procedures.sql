-- Procedure with various parameter types
CREATE PROCEDURE [dbo].[GetUserById]
    @UserId INT,
    @IncludeDeleted BIT = 0
AS
BEGIN
    SELECT @UserId AS UserId, @IncludeDeleted AS IncludeDeleted;
END
GO

-- Procedure with OUTPUT parameter
CREATE PROCEDURE [dbo].[CreateUser]
    @Name NVARCHAR(100),
    @Email NVARCHAR(255),
    @NewId INT OUTPUT
AS
BEGIN
    SET @NewId = 1;
    SELECT @Name AS Name, @Email AS Email;
END
GO

-- Procedure with many parameters (testing SqlSubroutineParameter count)
CREATE PROCEDURE [dbo].[ComplexProcedure]
    @Param1 INT,
    @Param2 NVARCHAR(100),
    @Param3 DECIMAL(18, 2),
    @Param4 DATETIME = NULL,
    @Param5 BIT = 0,
    @Param6 UNIQUEIDENTIFIER = NULL,
    @Param7 NVARCHAR(MAX) = '',
    @Param8 INT OUTPUT,
    @Param9 NVARCHAR(50) OUTPUT
AS
BEGIN
    SET @Param8 = @Param1 * 2;
    SET @Param9 = @Param2;
END
GO

-- Function with parameters
CREATE FUNCTION [dbo].[CalculateTotal]
(
    @Quantity INT,
    @UnitPrice DECIMAL(18, 2),
    @DiscountPercent DECIMAL(5, 2) = 0
)
RETURNS DECIMAL(18, 2)
AS
BEGIN
    RETURN @Quantity * @UnitPrice * (1 - @DiscountPercent / 100);
END
GO

-- Table-valued function with parameters
CREATE FUNCTION [dbo].[GetOrdersByCustomer]
(
    @CustomerId INT,
    @StartDate DATE = NULL,
    @EndDate DATE = NULL
)
RETURNS TABLE
AS
RETURN
(
    SELECT @CustomerId AS CustomerId, @StartDate AS StartDate, @EndDate AS EndDate
);
GO

CREATE PROCEDURE [dbo].[Procedure24]
    @Id INT,
    @Name NVARCHAR(100) = NULL,
    @Amount DECIMAL(18, 2) = 0
AS
BEGIN
    SET NOCOUNT ON;
    
    IF @Id IS NULL
    BEGIN
        SELECT [Id], [Name], [Description], [Amount], [Quantity], [IsActive], [CreatedDate]
        FROM [dbo].[Table25]
        WHERE [IsActive] = 1;
    END
    ELSE
    BEGIN
        SELECT [Id], [Name], [Description], [Amount], [Quantity], [IsActive], [CreatedDate]
        FROM [dbo].[Table25]
        WHERE [Id] = @Id;
    END
END
GO
